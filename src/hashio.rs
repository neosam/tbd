//! IO operations for hashable objects.
//!
//! # Usage
//! The main point here is to create objecs which can iteract with the HashIO
//! object.  These objects should be able to (de)serialize themselves, the children.
//! To do this, the HashIOImpl trait can be implemented and to be able to do this,
//! several the type must provide some features:
//!
//! * Hashable:  Create a hash which represents its content.
//! * Typeable:  Create a hash which represents its type.
//! * It should store a version number and the type hash and check against it when loading.
//!
//! This sounds complicated and that's why there are marcos which implement
//! everything required.


extern crate crypto;
extern crate byteorder;


use std::io::{Read, Write};
use std::{io, error, fmt};
use hash::*;
use io::*;
use std::fs::{File, create_dir_all};
use std::collections::BTreeMap;
use std::vec::Vec;
use std::path::Path;
use std::fs::rename;
use hashio_1;


/// Default error type for HashIO.
#[derive(Debug)]
pub enum HashIOError {
    Undefined(String),
    VersionError(u32),
    TypeError(Hash),
    IOError(io::Error),
    ParseError(Box<error::Error>)
}
impl fmt::Display for HashIOError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            HashIOError::Undefined(ref msg) => write!(f, "Undefined error: {}", msg),
            HashIOError::VersionError(version) => write!(f, "Unsupported version: {}", version),
            HashIOError::TypeError(ref hash) => write!(f, "Unexpected type: {}", hash.as_string()),
            HashIOError::IOError(ref err) => err.fmt(f),
            HashIOError::ParseError(ref err) => write!(f, "Parse error: {}", err)
        }
    }
}
impl error::Error for HashIOError {
    fn description(&self) -> &str {
        match *self {
            HashIOError::Undefined(ref msg) => msg,
            HashIOError::VersionError(_) => "Unsupported version",
            HashIOError::TypeError(_) => "Unexpected type",
            HashIOError::IOError(ref err) => err.description(),
            HashIOError::ParseError(ref err) => err.description()
        }
    }
}
impl From<io::Error> for HashIOError {
    fn from(err: io::Error) -> HashIOError {
        HashIOError::IOError(err)
    }
}


/// Structure to store and lead HashIO-able values
#[derive(Clone, Debug, PartialEq)]
pub struct HashIO {
    pub base_path: String,
    pub hash_io_1: hashio_1::HashIO1
}

/// Allows a type to identify itself.
pub trait Typeable {
    /// Identifies the type using a unique hash value.
    fn type_hash() -> Hash;
}

/// Trait makes sure the type can identify itself (the type) and it's content with hash values.
pub trait Hashtype : Hashable + Typeable {}

/// Makes a type HashIO-able.
///
/// A type needs to be able to load the hashable and to store it again.
pub trait HashIOImpl<T> where T: Hashtype {
    fn receive_hashable<R>(&self, read: &mut R, hash: &Hash) -> Result<T, HashIOError>
        where R: Read;
    fn store_hashable<W>(&self, hashable: &T, write: &mut W) -> Result<(), HashIOError>
        where W: Write;

    fn store_childs(&self, _: &T) -> Result<(), HashIOError> {
        Ok(())
    }
}

impl HashIO {
    pub fn new(path: String) -> HashIO {
        HashIO {
            base_path: path.clone(),
            hash_io_1: hashio_1::HashIO1::new(path)
        }
    }

    pub fn directory_for_hash(&self, hash: &Hash) -> String {
        let hash_str = hash.as_string();
        let mut result = String::new();
        result.push_str(&self.base_path);
        result.push('/');
        result.push_str(&hash_str[0..2]);
        result.push('/');
        result
    }

    pub fn filename_for_hash(&self, hash: &Hash) -> String {
        let hash_str = hash.as_string();
        let mut result = self.directory_for_hash(hash);
        result.push_str(&hash_str[2..]);
        result
    }

    pub fn get<T>(&self, hash: &Hash) -> Result<T, HashIOError>
                where HashIO: HashIOImpl<T>,
                      T: Hashtype {
        let filename = self.filename_for_hash(hash);
        let mut read = match File::open(filename.clone()) {
            Ok(r) => r,
            Err(err) => {
                print!("Could not load: {}\n", filename);
                return Err(HashIOError::from(err))
            }
        };
        let result : T = try!(self.receive_hashable(&mut read, hash));
        Ok(result)
    }

    pub fn put<T>(&self, hashable: &T) -> Result<(), HashIOError>
                where HashIO: HashIOImpl<T>,
                      T: Hashtype {
        let hash = hashable.as_hash();

        // First, if the entry already exists, skip the insert because it's already saved.
        let filename = self.filename_for_hash(&hash);
        if !Path::new(&filename).exists() {
            // First store all childs and their childs.
            // So we make sure that all dependencies are available when the current object has
            // finished writing.
            try!(self.store_childs(hashable));

            // First write in a slightly modified file which will be renamed when writing was
            // finished.  So we only have valid files or nothing on the expected position but
            // nothing unfinished.
            let safe_filename = format!("{}_", filename);
            let dir = self.directory_for_hash(&hash);
            try!(create_dir_all(dir));
            {
                let mut write = try!(File::create(Path::new(&safe_filename)));
                try!(self.store_hashable(hashable, &mut write));
                // 'write' will go out of scope now and so the file handle will be closed
            }
            try!(rename(safe_filename, filename));
        }
        Ok(())
    }
}


impl Typeable for String {
    fn type_hash() -> Hash {
        let id = String::from("String");
        let id_bytes = id.as_bytes();
        Hash::hash_bytes(id_bytes)
    }
}
impl Hashtype for String {}

impl HashIOImpl<String> for HashIO {
    fn store_hashable<W>(&self, hashable: &String, write: &mut W) -> Result<(), HashIOError>
                    where W: Write {
        try!(hashable.write_to(write));
        Ok(())
    }

    fn receive_hashable<R>(&self, read: &mut R, _: &Hash) -> Result<String, HashIOError>
                    where R: Read {
        let len = try!(read_u32(read));
        let bytes = try!(read_bytes(read, len as usize));
        let res = try!(String::from_utf8(bytes).map_err(|x| HashIOError::ParseError(Box::new(x))));
        Ok(res)
    }


}

pub fn flex_no<T>(_: &Hash, _: &HashIO, _: &hashio_1::HashIO1) -> Option<T> {
    None
}


/// Creates a convert function to from an old HashIO-able to a new one.
///
/// The From trait must be implemented for this.
///
/// Usage:  tbd_convert_gen!(new_function_name, OldType, NewType);
#[macro_export]
macro_rules! tbd_old_convert_gen {
    ($fn_name: ident, $old_type:ident, $new_type:ident) => {
        fn $fn_name(hash: &Hash, _: &HashIO, hash_io_1: &hashio_1::HashIO1) -> Option<$new_type> {
            let trial: Result<$old_type, hashio_1::HashIOError1> = hash_io_1.get(hash);
            match trial {
                Ok(res) => Some($new_type::from(res)),
                Err(_) => None
            }
        }
    }
}

/// Creates a convert function from one HashIO-able to another one.
///
/// The From trait must be implemented for this.
///
/// Usage:  tbd_convert_gen!(new_function_name, OldType, NewType);
#[macro_export]
macro_rules! tbd_convert_gen {
    ($fn_name: ident, $old_type:ty, $new_type:ty) => {
        fn $fn_name(hash: & Hash, hash_io: & HashIO, _: & hashio_1::HashIO1) -> Option < $new_type > {
            hash_io.get(hash).ok()
        }
    }
}

/// Chains several convert functions and creates a new one.
///
/// It will try all of them
///
/// Usage:  tbd_convert_chain!(new_function_name, NewType,
///             OldType1, OldType1, ...);
#[macro_export]
macro_rules! tbd_convert_chain {
    ($fn_name: ident, $new_type: ty, [
        $($convert_fn: ident),+
    ]) => {
        fn $fn_name(hash: &Hash, hash_io: &HashIO, hash_io_1: &hashio_1::HashIO1) -> Option<$new_type> {
            $(
                let option = $convert_fn(hash, hash_io, hash_io_1);
                if option.is_some() {
                    return option
                }
            )+
            return None
        }
    }
}

/// Creates a fully functional HashIO-able type.
#[macro_export]
macro_rules! tbd_model {
    //Old pattern calls the new pattern
    ($model_name:ident,
            [
                $([$attr_name:ident : $attr_type:ty, $exp_fn:ident, $imp_fn:ident ] ),*
            ],
            [
                $( [ $hash_name:ident : $hash_type:ident ] ),*
            ]) => {
                tbd_model!{
                    $model_name {
                        $([ $attr_name : $attr_type, $exp_fn, $imp_fn ]),*
                    } {
                        $( $hash_name : $hash_type ),*
                    }
                }
            };
    ($model_name:ident
            {
                $( [$attr_name:ident : $attr_type:ty, $exp_fn:ident, $imp_fn:ident ] ),*
            } {
                $( $hash_name:ident : $hash_type:ident
                    $(
                        <$($anno_type:ty),+>
                     )*
                ),*
            }) => {
                tbd_model!{
                    $model_name {
                        $( [$attr_name : $attr_type, $exp_fn, $imp_fn ] ),*
                    } {
                        $( $hash_name : $hash_type
                            $(
                                <$($anno_type),+>
                             )*
                        ),*
                    } {
                        flex_no
                    }
                }
            };

    ($model_name:ident
            {
                $( [$attr_name:ident : $attr_type:ty, $exp_fn:ident, $imp_fn:ident ] ),*
            } {
                $( $hash_name:ident : $hash_type:ident
                    $(
                        <$($anno_type:ty),+>
                     )*
                ),*
            } {
                $flex_type_fn:ident
            }) => {
        #[derive(Debug, Clone, PartialEq)]
        pub struct $model_name {
            $(pub $attr_name: $attr_type,)*
            $(pub $hash_name: $hash_type $(<$($anno_type),+>)*),*
        }

        impl $model_name {
            pub fn flex_fn(hash: &Hash, hash_io: &HashIO, hash_io_1: &hashio_1::HashIO1) -> Option<$model_name> {
                $flex_type_fn(hash, hash_io, hash_io_1)
            }

            pub fn internal_receive<R>(read: &mut R, _: &Hash, hash_io: &HashIO) -> Result<$model_name, HashIOError>
                    where R: Read {
                let version = try!(read_u32(read));
                if version < 1 {
                    return Err(HashIOError::VersionError(version))
                }
                let type_hash = try!(read_hash(read));
                if type_hash != $model_name::type_hash() {
                    return Err(HashIOError::TypeError(type_hash))
                }
                $( let $attr_name = try!($imp_fn(read)); )* ;
                $(
                    let $hash_name;
                    {
                        let hash_val = try!(read_hash(read));
                        $hash_name = try!(hash_io.get(&hash_val));
                    }
                )*
                Ok($model_name{
                    $($attr_name: $attr_name,)*
                    $($hash_name: $hash_name),*
                    })
            }
        }

        impl Typeable for $model_name {
            fn type_hash() -> Hash {
                let mut byte_gen: Vec<u8> = Vec::new();
                $(
                    {
                        let type_string = stringify!($attr_type);
                        let type_bytes = type_string.as_bytes();
                        let type_hash = Hash::hash_bytes(type_bytes);
                        byte_gen.extend_from_slice(&*type_hash.get_bytes());
                    };
                )*
                $(
                    {
                        let type_hash: Hash = $hash_type$(::<$($anno_type),+>)*::type_hash();
                        byte_gen.extend_from_slice(&*type_hash.get_bytes());
                    };
                )*
                Hash::hash_bytes(byte_gen.as_slice())
            }
        }

        impl Writable for $model_name {
            fn write_to<W: Write>(&self, write: &mut W) -> Result<usize, io::Error> {
                let mut size = 0;
                try!(write_u32(1, write));
                try!(write_hash(&$model_name::type_hash(), write));
                size += $( try!($exp_fn(self.$attr_name, write)); )*
                $(
                    try!(write_hash(&self.$hash_name.as_hash(), write));
                    size += 32;
                )*
                Ok(size)
            }
        }

        hashable_for_writable!($model_name);

        impl Hashtype for $model_name {}

        impl HashIOImpl<$model_name> for HashIO {
            fn receive_hashable<R>(&self, read: &mut R, hash: &Hash) -> Result<$model_name, HashIOError>
                            where R: Read {
                match $model_name::internal_receive(read, hash, self) {
                    Ok(res) => Ok(res),
                    Err(error) => {
                        print!("HashIO::receive_hash couldn't load type {} ({}) for hash {}: {}\n",
                            stringify!($model_name), $model_name::type_hash().as_string(),
                            hash.as_string(), error);
                        match $model_name::flex_fn(hash, self, &self.hash_io_1) {
                            None => Err(error),
                            Some(res) => Ok(res)
                        }
                    }
                }
            }

            fn store_childs(&self, hashable: &$model_name) -> Result<(), HashIOError> {
                $( try!(self.put(&hashable.$hash_name)); )*
                Ok(())
            }

            fn store_hashable<W>(&self, hashable: &$model_name, write: &mut W) -> Result<(), HashIOError>
                    where W: Write {
                try!(hashable.write_to(write));
                Ok(())
            }
        }
    }
}



#[cfg(test)]
mod test {
    use super::super::hash::*;
    use super::super::hashio::*;
    use super::super::io::*;
    use std::io::{Read, Write};

    #[derive(Debug)]
    struct A {
        a: u8,
        b: String
    }

    hashable_for_debug!(A);

    impl Typeable for A {
        fn type_hash() -> Hash {
            Hash::hash_string("A".to_string())
        }
    }
    impl Hashtype for A {}

    impl HashIOImpl<A> for HashIO {
        fn receive_hashable<R>(&self, read: &mut R, _: &Hash) -> Result<A, HashIOError>
                    where R: Read {
            let a = try!(read_u8(read));
            let b_hash = try!(read_hash(read));
            let b = try!(self.get(&b_hash));
            Ok(A{a: a, b: b})
        }

        fn store_childs(&self, hashable: &A) -> Result<(), HashIOError> {
            self.put(&hashable.b)
        }

        fn store_hashable<W>(&self, hashable: &A, write: &mut W) -> Result<(), HashIOError>
                    where W: Write {
            try!(write_u8(hashable.a, write));
            try!(write_hash(&hashable.b.as_hash(), write));
            Ok(())
        }
    }

    #[test]
    fn simple_test() {
        let hash_io = HashIO::new("unittest/savetest".to_string());
        let a_hash;
        {
            let a = A {
                a: 10,
                b: "Test".to_string()
            };
            a_hash = a.as_hash();
            hash_io.put(&a).unwrap();
        }

        let a2: A = hash_io.get(&a_hash).unwrap();
        assert_eq!(10, a2.a);
        assert_eq!("Test".to_string(), a2.b);
    }
}



#[cfg(test)]
mod test2 {
    use super::super::hash::*;
    use super::super::hashio::*;
    use super::super::io::*;
    use std::io::{Read, Write};
    use std::io;
    use super::super::hashio_1;

    tbd_model!(A, [
        [a: u8, write_u8, read_u8]
     ], [
        [b: String]
     ]);

    tbd_model!(B, [  ]
        , [
            [foo: String],
            [bar: A],
            [foobar: A]
        ]
    );

    #[test]
    fn simple_test() {
        let hash_io = HashIO::new("unittest/savetest".to_string());
        let my_hash;
        let b = B {
            foo: "Foo".to_string(),
            bar: A {
                a: 20,
                b: "Foo".to_string()
            },
            foobar: A {
                a: 30,
                b: "baz".to_string()
            }
        };
        my_hash = b.as_hash();
        hash_io.put(&b).unwrap();

        let b_read: B = hash_io.get(&my_hash).unwrap();
        assert_eq!(b, b_read);
        assert_eq!(b.foo, b_read.foo);
        assert_eq!(b.foobar, b_read.foobar);
    }
}


impl<T, U> Typeable for BTreeMap<T, U>
    where T: Hashtype, U: Hashtype,
          T: Ord {

    fn type_hash() -> Hash {
        let mut byte_gen: Vec<u8> = Vec::new();
        let id = String::from("BTreeMap");
        let id_bytes = id.as_bytes();
        byte_gen.extend_from_slice(&*Hash::hash_bytes(id_bytes).get_bytes());
        byte_gen.extend_from_slice(&*T::type_hash().get_bytes());
        byte_gen.extend_from_slice(&*U::type_hash().get_bytes());
        Hash::hash_bytes(byte_gen.as_slice())
    }
}
impl<T: Hashtype + Writable + Ord,
     U: Hashtype + Writable> Hashtype for BTreeMap<T, U> {}

impl<T, U> HashIOImpl<BTreeMap<T, U>> for HashIO
    where HashIO: HashIOImpl<T>,
          HashIO: HashIOImpl<U>,
          T: Writable, U: Writable,
          T: Hashtype, U: Hashtype,
          T: Ord {
    fn store_hashable<W>(&self, hashable: &BTreeMap<T, U>, write: &mut W) -> Result<(), HashIOError>
        where W: Write {
        for (key, value) in hashable {
            try!(self.put(key));
            try!(self.put(value));
        }
        try!(hashable.write_to(write));
        Ok(())
    }

    fn receive_hashable<R>(&self, read: &mut R, _: &Hash) -> Result<BTreeMap<T, U>, HashIOError>
        where R: Read {
        let mut res = BTreeMap::<T, U>::new();
        try!(read_u32(read));
        let entries = try!(read_u32(read));
        for _ in 0..entries {
            let key_hash = try!(read_hash(read));
            let value_hash = try!(read_hash(read));
            let key = try!(self.get(&key_hash));
            let value = try!(self.get(&value_hash));
            res.insert(key, value);
        }
        Ok(res)
    }

}

#[cfg(test)]
mod btreemaptest {
    use super::super::hash::*;
    use super::super::hashio::*;
    use super::super::io::*;
    use std::io::{Read, Write};
    use std::io;
    use std::collections::BTreeMap;
    use hashio_1;

    tbd_model!{
        A {} {
            a: BTreeMap<String, String>
        }
    }
    #[test]
    fn test() {
        let hash_io = HashIO::new("unittest/btreemaptest".to_string());
        let mut a = A { a: BTreeMap::new() };
        a.a.insert("one".to_string(), "1".to_string());
        a.a.insert("two".to_string(), "2".to_string());
        let hash = a.as_hash();
        hash_io.put(&a).unwrap();
        let a_2 = hash_io.get(&hash).unwrap();
        assert_eq!(a, a_2);
    }
}

impl<T> Typeable for Vec<T>
    where T: Hashable, T: Typeable {

    fn type_hash() -> Hash {
        let mut byte_gen: Vec<u8> = Vec::new();
        let id = String::from("Vec");
        let id_bytes = id.as_bytes();
        byte_gen.extend_from_slice(&*Hash::hash_bytes(id_bytes).get_bytes());
        byte_gen.extend_from_slice(&*T::type_hash().get_bytes());
        Hash::hash_bytes(byte_gen.as_slice())
    }
}
impl<T: Hashable + Typeable + Writable> Hashtype for Vec<T> {}

impl<T> HashIOImpl<Vec<T>> for HashIO
    where HashIO: HashIOImpl<T>,
          T: Writable, T: Hashtype {
    fn store_hashable<W>(&self, hashable: &Vec<T>, write: &mut W) -> Result<(), HashIOError>
        where W: Write {
        for value in hashable {
            try!(self.put(value));
        }
        try!(hashable.write_to(write));
        Ok(())
    }

    fn receive_hashable<R>(&self, read: &mut R, _: &Hash) -> Result<Vec<T>, HashIOError>
        where R: Read {
        let mut res = Vec::<T>::new();
        try!(read_u32(read));
        let entries = try!(read_u32(read));
        for _ in 0..entries {
            let value_hash = try!(read_hash(read));
            let value = try!(self.get(&value_hash));
            res.push(value);
        }
        Ok(res)
    }


}


#[cfg(test)]
mod convert_test {
    use super::super::hash::*;
    use super::super::hashio::*;
    use super::super::hashio_1::*;
    use super::super::io::*;
    use std::io::{Read, Write};
    use std::io;
    use std::fs::remove_dir_all;
    use hashio_1;

    tbd_model_1!(A1, [], [
        [x: String]]
    );

    tbd_model! {
        B {} {
            b: String,
            y: String
        }
    }

    tbd_model! {
        A {} {
            x: String,
            y: String
        } {
            a_convert
        }
    }
    impl From<A1> for A {
        fn from(a1: A1) -> A {
            A {
                x: a1.x,
                y: "".to_string()
            }
        }
    }
    impl From<B> for A {
        fn from(b: B) -> A {
            A {
                x: b.b,
                y: b.y
            }
        }
    }
    tbd_old_convert_gen!(a_old_convert, A1, A);
    tbd_convert_gen!(a_b_convert, B, A);
    tbd_convert_chain!(a_convert, A, [a_old_convert, a_b_convert]);


    fn save_hashio1() -> Hash {
        let hash_io = HashIO1::new("./unittest/convert_test/".to_string());
        let a = A1{x: "bla".to_string()};
        let hash = a.as_hash();
        hash_io.put(&a).unwrap();
        hash
    }
    fn save_b() -> Hash {
        let hash_io = HashIO::new("unittest/convert_test/".to_string());
        let b = B {b: "bla".to_string(), y: "".to_string()};
        let hash = b.as_hash();
        hash_io.put(&b).unwrap();
        hash
    }

    fn load_a(hash: &Hash) -> Option<A> {
        let hash_io = HashIO::new("./unittest/convert_test/".to_string());
        hash_io.get::<A>(hash).ok()
    }

    fn load_b(hash: &Hash) -> Option<A> {
        let hash_io = HashIO::new("./unittest/convert_test/".to_string());
        hash_io.get::<A>(hash).ok()
    }

    fn save_a(a: &A) {
        let hash_io = HashIO::new("./unittest/convert_test/".to_string());
        hash_io.put(a).unwrap();
    }

    #[test]
    fn main() {
        remove_dir_all("./unittest/convert_test/").ok();
        let hash1 = save_hashio1();
        let hash_b = save_b();
        let a = load_a(&hash1).unwrap();
        assert_eq!("bla".to_string(), a.x);
        assert_eq!("".to_string(), a.y);
        let hash = a.as_hash();
        save_a(&a);
        let a_again = load_a(&hash).unwrap();
        assert_eq!("bla".to_string(), a_again.x);
        assert_eq!("".to_string(), a_again.y);

        let a_again = load_b(&hash_b).unwrap();
    }
}