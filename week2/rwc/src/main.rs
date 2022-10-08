use std::env;
use std::process;
use std::fs::File;
use std::io::{self, BufRead};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Too few arguments.");
        process::exit(1);
    }
    let filename = &args[1];
    let mut res = Vec::new();
    let mut num_lines = 0;
    let mut num_words = 0;
    let mut num_chars = 0;
    let file = File::open(filename).unwrap();
    for line in io::BufReader::new(file).lines() {
        let line_str = line.unwrap();
        num_lines += 1;
        num_words += line_str.split(' ').collect::<Vec<&str>>().len();
        num_chars += line_str.len();
        res.push(line_str);
    }
    println!("lines:{}", num_lines);
    println!("words:{}", num_words);
    println!("chars:{}", num_chars);
}
