// Simple Hangman Program
// User gets five incorrect guesses
// Word chosen randomly from words.txt
// Inspiration from: https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html
// This assignment will introduce you to some fundamental syntax in Rust:
// - variable declaration
// - string manipulation
// - conditional statements
// - loops
// - vectors
// - files
// - user input
// We've tried to limit/hide Rust's quirks since we'll discuss those details
// more in depth in the coming lectures.
extern crate rand;
use rand::Rng;
use std::fs;
use std::io;
use std::io::Write;

const NUM_INCORRECT_GUESSES: u32 = 5;
const WORDS_PATH: &str = "words.txt";

fn pick_a_random_word() -> String {
    let file_string = fs::read_to_string(WORDS_PATH).expect("Unable to read file.");
    let words: Vec<&str> = file_string.split('\n').collect();
    String::from(words[rand::thread_rng().gen_range(0, words.len())].trim())
}

fn main() {
    let secret_word = pick_a_random_word();
    // Note: given what you know about Rust so far, it's easier to pull characters out of a
    // vector than it is to pull them out of a string. You can get the ith character of
    // secret_word by doing secret_word_chars[i].
    let secret_word_chars: Vec<char> = secret_word.chars().collect();
    // Uncomment for debugging:
    // println!("random word: {}", secret_word);

    // Your code here! :)
    let mut now_word_chars: Vec<char> = Vec::new();
    let mut guess_word: String = "".to_string();
    let mut loss_num = NUM_INCORRECT_GUESSES; 
    let mut guessed_word_indexs: Vec<bool> = Vec::new();
    for _ in 0..secret_word_chars.len() {
        now_word_chars.push('-');
        guessed_word_indexs.push(false);
    }
    println!("Welcome to CS110L Hangman!");
    loop {
        let s: String = now_word_chars.iter().collect();
        let mut guess = String::new();
        let mut flag = false;
        println!("The word so far is {}", s);
        println!("You have guessed the following letters:{}", guess_word);
        println!("You have {} guesses left", loss_num);
        println!("Please guess a letter: ");
        io::stdout().flush().expect("Error flushing stdout.");
        io::stdin().read_line(&mut guess).expect("Error reading line.");
        if guess.len() > 2 {
            panic!("guess is too long");
        }
        let guess_chars: Vec<char> = guess.chars().collect();
        guess_word.push(guess_chars[0]);
        for (i, ele) in secret_word_chars.iter().enumerate() {
            if (*ele == guess_chars[0]) && (guessed_word_indexs[i] == false) {
                flag = true;
                now_word_chars[i] = *ele;
                guessed_word_indexs[i] = true;
                break;
            }
        }
        if flag == false {
            println!("Sorry, that letter is not in the word");
            loss_num -= 1;
        }
        println!("");
        if now_word_chars == secret_word_chars {
            println!("Congratulations you guessed the secret word: {}!", secret_word);
            break;
        }
        if loss_num == 0 {
            println!("Sorry, you ran out of guesses!");
            break;
        }
    }
}
