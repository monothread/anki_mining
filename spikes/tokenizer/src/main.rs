use std::time::Instant;

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera_analysis::tokenizer::Tokenizer;

const SENTENCES: [&str; 8] = [
    "猫が好きです",
    "昨日、友達と映画を見に行きました。",
    "食べさせられたくなかった",
    "この本は読みやすいと思います",
    "彼女は先生になりたがっている",
    "東京都へ行く",
    "日本語を勉強しています",
    "美しかったので買ってしまった",
];

fn main() {
    for name in ["unidic", "ipadic"] {
        let started = Instant::now();
        let dictionary = load_dictionary(&format!("embedded://{name}")).expect("dictionary");
        let tokenizer = Tokenizer::new(Segmenter::new(Mode::Normal, dictionary, None));
        println!("=== {name} (load {:?})", started.elapsed());
        for sentence in SENTENCES {
            println!("-- {sentence}");
            let mut tokens = tokenizer.tokenize(sentence).expect("tokenize");
            for token in tokens.iter_mut() {
                let surface = token.surface.to_string();
                println!("   {}\t{}", surface, token.details().join(","));
            }
        }
        let started = Instant::now();
        for _ in 0..1000 {
            for sentence in SENTENCES {
                let _ = tokenizer.tokenize(sentence).expect("tokenize");
            }
        }
        println!("8000 sentences in {:?}", started.elapsed());
    }
}
