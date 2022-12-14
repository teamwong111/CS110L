mod request;
mod response;
use std::{sync::Arc, collections::HashMap};
use clap::Parser;
use rand::{Rng, SeedableRng};
use tokio::{net::{TcpListener, TcpStream}, stream::StreamExt};
use tokio::sync::{RwLock, Mutex};
use std::io::ErrorKind;
use std::time::Duration;
use tokio::time::delay_for;

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[clap(about = "Fun with load balancing")]
struct CmdOptions {
    #[clap(
        short,
        long,
        help = "IP/port to bind to",
        default_value = "0.0.0.0:1100"
    )]
    bind: String,
    #[clap(short, long, help = "Upstream host to forward requests to")]
    upstream: Vec<String>,
    #[clap(
        long,
        help = "Perform active health checks on this interval (in seconds)",
        default_value = "10"
    )]
    active_health_check_interval: usize,
    #[clap(
    long,
    help = "Path to send request to for active health checks",
    default_value = "/"
    )]
    active_health_check_path: String,
    #[clap(
        long,
        help = "Maximum number of requests to accept per IP per minute (0 = unlimited)",
        default_value = "0"
    )]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    #[allow(dead_code)]
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    #[allow(dead_code)]
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    #[allow(dead_code)]
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: Vec<String>,
    failed_indexs: RwLock<HashMap<usize, bool>>,
    rate_limit_counter: Mutex<HashMap<String, usize>>,
}

#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let mut listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = ProxyState {
        upstream_addresses: options.upstream,
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
        failed_indexs: RwLock::new(HashMap::new()),
        rate_limit_counter: Mutex::new(HashMap::new()),
    };
    let shared_state = Arc::new(state);
    let state_clone = shared_state.clone();
    tokio::spawn(async move {
        _ = active_health_checks(&state_clone).await;
    });

    if shared_state.max_requests_per_minute > 0 {
        let state_clone1 = shared_state.clone();
        tokio::spawn(async move {
            rate_limit_counter_refresher(&state_clone1, 60).await;
        });
    }

    let mut incoming = listener.incoming();
    while let Some(stream) = incoming.next().await {
        if let Ok(stream) = stream {
            // Handle the connection!
            let shared_state = shared_state.clone();
            tokio::spawn(async move {
                handle_connection(stream, &shared_state).await;
            });
        }
    }
}

async fn rate_limit_counter_refresher(state: &ProxyState, interval: u64) {
    delay_for(Duration::from_secs(interval)).await;
    let mut rate_limit_counter = state.rate_limit_counter.lock().await;
    rate_limit_counter.clear();
}

async fn connect_to_deterministic_upstream(upstream_idx: usize, state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    let upstream_ip = &state.upstream_addresses[upstream_idx];
    match TcpStream::connect(upstream_ip).await {
        Ok(stream) => return Ok(stream),
        Err(err) => {
            log::error!("Failed to connect to upstream {}: {}", upstream_ip, err);
            return Err(err);
        }
    }
}

async fn active_health_checks(state: &ProxyState) -> Result<(), std::io::Error> {
    loop {
        delay_for(Duration::from_secs(state.active_health_check_interval as u64)).await;
        let mut wfi = state.failed_indexs.write().await;
        for i in 0..state.upstream_addresses.len() {
            let mut stream = connect_to_deterministic_upstream(i, &state).await?;
            let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(&state.active_health_check_path)
            .header("Host", &state.upstream_addresses[i])
            .body(Vec::new())
            .unwrap();
            request::write_to_stream(&request, &mut stream).await?;
            match response::read_from_stream(&mut stream, &http::Method::GET).await {
                Err(_) => {}
                Ok(res) => {
                    if res.status().as_u16() == 200 {
                        wfi.remove(&i);
                    }
                    else {
                        wfi.insert(i, true);
                    }
                }
            }
        }
    }
}

async fn get_upstream_idx(state: &ProxyState) -> Option<usize> {
    let mut rng = rand::rngs::StdRng::from_entropy();
    let rfi = state.failed_indexs.read().await;
    if rfi.len() == state.upstream_addresses.len() { 
        return None;
    }
    let mut upstream_idx;
    loop {
        upstream_idx = rng.gen_range(0, state.upstream_addresses.len());
        if rfi.get(&upstream_idx).is_none() { 
            break;
        }
    }
    Some(upstream_idx)
}

async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    loop {
        if let Some(upstream_idx) = get_upstream_idx(&state).await {
            let upstream_ip = &state.upstream_addresses[upstream_idx];
            match TcpStream::connect(upstream_ip).await {
                Ok(stream) => { return Ok(stream) }
                Err(err) => {
                    log::error!("Failed to connect to upstream {}: {}", upstream_ip, err);
                    let mut wfi = state.failed_indexs.write().await;
                    wfi.insert(upstream_idx, true);
                }
            }
        }
        else {
            return Err(std::io::Error::new(ErrorKind::Other, "All upstream servers failed!"));
        }
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("{} <- {}", client_ip, response::format_response_line(&response));
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = client_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        let mut rate_limit_counter = state.rate_limit_counter.lock().await;
        let ip = client_conn.peer_addr().unwrap().ip().to_string();
        let count = rate_limit_counter.entry(ip).or_insert(0);
        *count += 1;
        if *count > state.max_requests_per_minute {
            let response = response::make_http_error(http::StatusCode::TOO_MANY_REQUESTS);
            response::write_to_stream(&response, &mut client_conn).await.unwrap();
            continue;
        }

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!("Failed to send request to upstream {}: {}", upstream_ip, error);
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
