use std::collections::HashMap;
use std::io::{self, Read};

pub struct Cache<T> {
    data: HashMap<String, T>,
    capacity: usize,
}

impl<T: Clone> Cache<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: HashMap::new(),
            capacity,
        }
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.data.get(key)
    }

    pub fn insert(&mut self, key: String, value: T) -> bool {
        if self.data.len() >= self.capacity && !self.data.contains_key(&key) {
            return false;
        }
        self.data.insert(key, value);
        true
    }

    pub fn remove(&mut self, key: &str) -> Option<T> {
        self.data.remove(key)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

pub trait Serializable {
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(data: &[u8]) -> io::Result<Self> where Self: Sized;
}

pub enum Format {
    Json,
    Binary,
    Text,
}

fn read_input() -> io::Result<String> {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

pub fn process_entries(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| !e.is_empty())
        .map(|e| e.trim().to_lowercase())
        .collect()
}
// line 66
// line 67
// line 68
// line 69
// line 70
// line 71
// line 72
// line 73
// line 74
// line 75
// line 76
// line 77
// line 78
// line 79
// line 80
// line 81
// line 82
// line 83
// line 84
// line 85
