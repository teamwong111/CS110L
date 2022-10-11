use crossbeam_channel;
use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());
    output_vec.resize_with(input_vec.len(), Default::default);
    let (isender, ireceiver) = crossbeam_channel::unbounded();
    let (osender, oreceiver) = crossbeam_channel::unbounded();
    let mut threads = Vec::new();
    for _ in 0..num_threads {
        let ireceiver = ireceiver.clone();
        let osender = osender.clone();
        threads.push(thread::spawn(move || {
            while let Ok(next_msg) = ireceiver.recv() {
                let (index, value) = next_msg;
                osender.send((index, f(value))).expect("there are no receivers!");
            }
        }));
    }
    let len = input_vec.len();
    for i in 0..len {
        isender.send((len - i - 1, input_vec.pop().unwrap())).expect("there is no receiver");
    }
    drop(isender);
    drop(osender);
    while let Ok(next_msg) = oreceiver.recv() {
        let (index, value) = next_msg;
        output_vec[index] = value;
    }
    for thread in threads {
        thread.join().expect("Panic occurred in thread");
    }
    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
