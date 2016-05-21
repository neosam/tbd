//! Log entries chained by crypto hashes.
//!
//! # Usage
//! As the most abstract type, the main type is the Log trait in
//! this module.  It has a basic implementation DefaultLog which
//! implements all of its features (of course) and additionally
//! provides the ability to define custom load and save closures
//! which can be used to save and load entries.  By default, they
//! are empty.
//!
//! Since all entries are chained by their crypto hashes, the
//! Hashable trait must be implemented for the types.  It's also
//! possible to implement the Writable trait which provides a
//! a helper function to calculate the hash.
//!
//! # Examples
//!
//! ```
//! use tbd::log::*;
//! use std::io::Write;
//!
//! // Defining the type we want to store.
//! struct MyStruct {
//!    x: u8
//! }
//!
//! // Implement the Writable trait
//! impl Writable for MyStruct {
//!    fn write_to(&self, write: &mut Write) {
//!       let byte = [self.x];
//!       write.write(&byte);
//!    }
//! }
//!
//! // Use the helper function writeable_to_hash to implment as_hash()
//! impl Hashable for MyStruct {
//!    fn as_hash(&self) -> Hash {
//!       self.writeable_to_hash()
//!    }
//! }
//!
//! // Create new log
//! let mut log = DefaultLog::<MyStruct>::new();
//!
//! // Add some entries
//! let first_hash: Hash = log.push(MyStruct{x: 42});
//! let second_hash: Hash = log.push(MyStruct{x: 23});
//!
//! // The push method returns the hash value which can be used as key.
//! assert_eq!("377194384a7432ebd8d8e0f19a1bcc17f115a220d48e280f8d75b6a5b43c3e1d",
//!                &first_hash.as_string());
//! assert_eq!("5894a38091d60a64cb6396edc2662c6460c3685b78b4381051dbc15ff30c5bcc",
//!                &second_hash.as_string());
//!
//! // Inserting the same value again gives a completely different hash because
//! // the hash also contains the privious entry.
//! let third_hash: Hash = log.push(MyStruct{x: 23});
//! assert_eq!("f87fa51292d72bb55a842b3f46c83adf71720a89abc3c7d89494d84458b57861",
//!                &third_hash.as_string());
//!
//! // Verify entries
//! assert_eq!(42, log.get(first_hash).unwrap().x);
//! assert_eq!(23, log.get(second_hash).unwrap().x);
//! assert_eq!(23, log.get(third_hash).unwrap().x);
//!
//! // Iterate over the entries
//! // This log operates like a stack and will return the last (latest)
//! // entry first.
//! let mut res = Vec::<u8>::new();
//! for item in log.iter() {
//!     res.push(item.x);
//! }
//!
//! assert_eq!(23, res[0]);
//! assert_eq!(23, res[1]);
//! assert_eq!(42, res[2]);
//! ```


extern crate time;
extern crate crypto;
extern crate byteorder;

use std::io::{Write};
use self::crypto::sha3::Sha3;
use self::crypto::digest::Digest;
use std::collections::BTreeMap;


// ---- Core types ----

/// Contains a ordered set of entries and use a way to verify them.
pub trait Log {
    type Item: Hashable;

    /// Create a new log
    fn new() -> Self;

    /// Add new entry to the log
    fn push(&mut self, Self::Item) -> Hash;


    /// Head hash
    fn head_hash(&self) -> Option<Hash>;

    /// Get the parent hash of the given hash
    fn parent_hash(&self, hash: Hash) -> Option<Hash>;

    /// Get the borrowed entry of the given hash
    fn get(&self, hash: Hash) -> Option<&Self::Item>;

    /// Get a mutable entry of the given hash
    fn get_mut(&mut self, hash: Hash) -> Option<&mut Self::Item>;
}


pub struct LogIteratorRef<'a, L: Log<Item=T> + 'a, T: Hashable> {
    log: &'a L,
    hash: Option<Hash>
}

impl<'a, L: Log<Item=T>, T: Hashable + 'a> LogIteratorRef<'a, L, T> {
    pub fn from_log(log: &'a L) -> LogIteratorRef<'a, L, T> {
        LogIteratorRef {
            log: log,
            hash: log.head_hash()
        }
    }
}

impl<'a, L: Log<Item=T>, T: Hashable + 'a> Iterator for LogIteratorRef<'a, L, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        match self.hash {
            None => None,
            Some(hash) => {
                let value = self.log.get(hash);
                self.hash = self.log.parent_hash(hash);
                value
            }
        }
    } 
}


pub struct LogIteratorHash<'a, L: Log<Item=T> + 'a, T: Hashable> {
    log: &'a L,
    hash: Option<Hash>
}

impl<'a, L: Log<Item=T>, T: Hashable + 'a> LogIteratorHash<'a, L, T> {
    pub fn from_log(log: &'a L) -> LogIteratorHash<'a, L, T> {
        LogIteratorHash {
            log: log,
            hash: log.head_hash()
        }
    }
}

impl<'a, L: Log<Item=T>, T: Hashable + 'a> Iterator for LogIteratorHash<'a, L, T> {
    type Item = Hash;

    fn next(&mut self) -> Option<Hash> {
        match self.hash {
            None => None,
            Some(hash) => {
                let value = self.log.get(hash);
                self.hash = self.log.parent_hash(hash);
                self.hash
            }
        }
    } 
}


// ---- Defining HashLog types

/// Stores one of the supported hash values.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hash {
    None,
    Sha3([u8; 32])
}

fn half_byte_to_string(byte: u8) -> String {
    match byte {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        10 => "a",
        11 => "b",
        12 => "c",
        13 => "d",
        14 => "e",
        15 => "f",
        _ => "?"
    }.to_string()
}

fn byte_to_string(byte: u8) -> String {
    let mut res = String::new();
    res.push_str(&half_byte_to_string(byte / 16));
    res.push_str(&half_byte_to_string(byte % 16));
    res
}

fn bytes_to_string(bytes: &[u8]) -> String {
    let mut res = String::new();
    for byte in bytes {
        res.push_str(&byte_to_string(*byte)); 
    }
    res
}

impl Hash {
    /// Get the hash as byte array.
    pub fn get_bytes(&self) -> Box<[u8]>{
        match self {
            &Hash::None => Box::new([0u8;0]),
            &Hash::Sha3(x) => Box::new(x)
        }
    }

    pub fn as_string(&self) -> String {
        bytes_to_string(&*self.get_bytes())
    }

    pub fn hash_bytes(bytes: &[u8]) -> Hash {
        let mut sha3 = Sha3::sha3_256();
        sha3.input(bytes);
        let mut res = [0u8; 32];
        sha3.result(&mut res);
        Hash::Sha3(res)
    }

    pub fn hash_with(&self, o: Hash) -> Hash {
        let mut vec: Vec<u8> = Vec::new();
        vec.extend_from_slice(&*self.get_bytes());
        vec.extend_from_slice(&*o.get_bytes());
        Hash::hash_bytes(vec.as_slice())
    }
}

/// Can generate a hash type which represents the current type.
pub trait Hashable {
    fn as_hash(&self) -> Hash;
}

impl Hashable for Hash {
    fn as_hash(&self) -> Hash {
        Hash::hash_bytes(&*self.get_bytes())
    }
}

pub struct DefaultLogEntry<T: Hashable> {
    entry: T,
    parent_hash: Option<Hash>
}


pub struct DefaultLog<T: Hashable> {
    entries: BTreeMap<Hash, DefaultLogEntry<T>>,
    head: Option<Hash>,
    load: Box<Fn(Hash) -> Option<DefaultLogEntry<T>>>,
    save: Box<Fn(&DefaultLogEntry<T>)>
}

impl<T: Hashable> DefaultLog<T> {
    pub fn iter(&self) -> LogIteratorRef<DefaultLog<T>, T> {
        LogIteratorRef::from_log(self)
    }

    pub fn hash_iter(&self) -> LogIteratorHash<DefaultLog<T>, T> {
        LogIteratorHash::from_log(self)
    }

    pub fn with_load_fn(mut self, load_fn: Box<Fn(Hash) -> Option<DefaultLogEntry<T>>>) -> DefaultLog<T> {
        self.load = load_fn;
        self
    }

    pub fn with_save_fn(mut self, save_fn: Box<Fn(&DefaultLogEntry<T>)>) -> DefaultLog<T> {
        self.save = save_fn;
        self
    }
}

impl<T: Hashable> Log for DefaultLog<T> {
    type Item = T;

    fn new() -> Self {
        DefaultLog {
            entries: BTreeMap::new(),
            head: None,
            load: Box::new(|_| None),
            save: Box::new(|_| ())
        }
    }

    fn push(&mut self, t: T) -> Hash {
        let entry_hash = t.as_hash();
        let hash = match self.head {
            None => entry_hash.as_hash(),
            Some(head_hash) => entry_hash.hash_with(head_hash)
        };
        let log_entry = DefaultLogEntry {
            entry: t,
            parent_hash: self.head
        };
        self.entries.insert(hash, log_entry);
        self.head = Some(hash);
        hash
    }

    fn head_hash(&self) -> Option<Hash> {
        self.head
    }

    fn parent_hash(&self, hash: Hash) -> Option<Hash> {
        match self.entries.get(&hash) {
            None => None,
            Some(ref entry) => entry.parent_hash
        }
    }

    fn get(&self, hash: Hash) -> Option<&Self::Item> {
        match self.entries.get(&hash) {
            None => None,
            Some(ref entry) => Some(&entry.entry)
        }
    }

    fn get_mut(&mut self, hash: Hash) -> Option<&mut Self::Item> {
        match self.entries.get_mut(&hash) {
            None => None,
            Some(entry) => Some(&mut entry.entry)
        }
    }
}


// ---- Defining WritableLog types ----

/// Write itself to any write trait.
///
/// It also implements the Hashable type by default and generates
/// a sha3 representation of its output.
pub trait Writable: Hashable {
    fn write_to(&self, write: &mut Write);
    fn writeable_to_hash(&self) -> Hash {
        let mut write: Vec<u8> = Vec::new();
        self.write_to(&mut write);
        let data = write.as_slice();
        let mut hasher = Sha3::sha3_256();
        let mut hash_bytes = [0u8; 32];
        hasher.input(data);
        hasher.result(&mut hash_bytes);
        let hash = Hash::Sha3(hash_bytes);
        hash
    }
}



