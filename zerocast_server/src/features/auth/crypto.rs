use rand::{RngExt, distr::Alphanumeric};

pub fn generate_random_string(length: usize) -> String {
  rand::rng()
    .sample_iter(&Alphanumeric)
    .take(length)
    .map(char::from)
    .collect()
}

pub fn generate_numeric_code(length: usize) -> String {
  (0..length)
    .map(|_| rand::random_range(0..10).to_string())
    .collect()
}
