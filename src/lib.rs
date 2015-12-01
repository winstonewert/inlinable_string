// Copyright 2015, The inlinable_string crate Developers. See the COPYRIGHT file
// at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT
// or http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! The `inlinable_string` crate provides the
//! [`InlinableString`](./enum.InlinableString.html) type &mdash; an owned,
//! grow-able UTF-8 string that stores small strings inline and avoids
//! heap-allocation &mdash; and the
//! [`StringExt`](./string_ext/trait.StringExt.html) trait which abstracts
//! string operations over both `std::string::String` and `InlinableString` (or
//! even your own custom string type).
//!
//! `StringExt`'s API is mostly identical to `std::string::String`; unstable and
//! deprecated methods are not included. A `StringExt` implementation is
//! provided for both `std::string::String` and `InlinableString`. This enables
//! `InlinableString` to generally work as a drop-in replacement for
//! `std::string::String` and `&StringExt` to work with references to either
//! type.
//!
//! # Examples
//!
//! ```
//! use inlinable_string::{InlinableString, StringExt};
//!
//! // Small strings are stored inline and don't perform heap-allocation.
//! let mut s = InlinableString::from("small");
//! assert_eq!(s.capacity(), inlinable_string::INLINE_STRING_CAPACITY);
//!
//! // Inline strings are transparently promoted to heap-allocated strings when
//! // they grow too big.
//! s.push_str("a really long string that's bigger than `INLINE_STRING_CAPACITY`");
//! assert!(s.capacity() > inlinable_string::INLINE_STRING_CAPACITY);
//!
//! // This method can work on strings potentially stored inline on the stack,
//! // on the heap, or plain old `std::string::String`s!
//! fn takes_a_string_reference(string: &mut StringExt) {
//!    // Do something with the string...
//!    string.push_str("it works!");
//! }
//!
//! let mut s1 = String::from("this is a plain std::string::String");
//! let mut s2 = InlinableString::from("inline");
//!
//! // Both work!
//! takes_a_string_reference(&mut s1);
//! takes_a_string_reference(&mut s2);
//! ```
//!
//! # Porting Your Code
//!
//! * If `my_string` is always on the stack: `let my_string = String::new();` →
//! `let my_string = InlinableString::new();`
//!
//! * `fn foo(string: &mut String) { ... }` → `fn foo(string: &mut StringExt) { ... }`
//!
//! * `fn foo(string: &str) { ... }` does not need to be modified.
//!
//! * `struct S { member: String }` is a little trickier. If `S` is always stack
//! allocated, it probably makes sense to make `member` be of type
//! `InlinableString`. If `S` is heap-allocated and `member` is *always* small,
//! consider using the more restrictive
//! [`InlineString`](./inline_string/struct.InlineString.html) type. If `member` is
//! not always small, then it should probably be left as a `String`.

#![forbid(missing_docs)]

#![cfg_attr(feature = "nightly", feature(plugin))]
#![cfg_attr(feature = "nightly", plugin(clippy))]
#![cfg_attr(feature = "nightly", deny(clippy))]

#![cfg_attr(all(test, feature = "nightly"), feature(test))]

#[cfg(test)]
#[cfg(feature = "nightly")]
extern crate test;

pub mod inline_string;
pub mod string_ext;

pub use inline_string::{INLINE_STRING_CAPACITY, InlineString};
pub use string_ext::StringExt;

use std::borrow::{Borrow, Cow};
use std::fmt;
use std::hash;
use std::iter;
use std::mem;
use std::ops;
use std::string::{FromUtf8Error, FromUtf16Error};

/// An owned, grow-able UTF-8 string that allocates short strings inline on the
/// stack.
///
/// See the [module level documentation](./index.html) for more.
#[derive(Clone, Debug, Eq)]
pub enum InlinableString {
    /// A heap-allocated string.
    Heap(String),
    /// A small string stored inline.
    Inline(InlineString),
}

impl iter::FromIterator<char> for InlinableString {
    fn from_iter<I: IntoIterator<Item=char>>(iter: I) -> InlinableString {
        let mut buf = InlinableString::new();
        buf.extend(iter);
        buf
    }
}

impl<'a> iter::FromIterator<&'a str> for InlinableString {
    fn from_iter<I: IntoIterator<Item=&'a str>>(iter: I) -> InlinableString {
        let mut buf = InlinableString::new();
        buf.extend(iter);
        buf
    }
}

impl Extend<char> for InlinableString {
    fn extend<I: IntoIterator<Item=char>>(&mut self, iterable: I) {
        let iterator = iterable.into_iter();
        let (lower_bound, _) = iterator.size_hint();
        self.reserve(lower_bound);
        for ch in iterator {
            self.push(ch);
        }
    }
}

impl<'a> Extend<&'a char> for InlinableString {
    fn extend<I: IntoIterator<Item=&'a char>>(&mut self, iter: I) {
        self.extend(iter.into_iter().cloned());
    }
}

impl<'a> Extend<&'a str> for InlinableString {
    fn extend<I: IntoIterator<Item=&'a str>>(&mut self, iterable: I) {
        let iterator = iterable.into_iter();
        let (lower_bound, _) = iterator.size_hint();
        self.reserve(lower_bound);
        for s in iterator {
            self.push_str(s);
        }
    }
}

impl<'a> ops::Add<&'a str> for InlinableString {
    type Output = InlinableString;

    #[inline]
    fn add(mut self, other: &str) -> InlinableString {
        self.push_str(other);
        self
    }
}

impl hash::Hash for InlinableString {
    #[inline]
    fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
        (**self).hash(hasher)
    }
}

impl Borrow<str> for InlinableString {
    fn borrow(&self) -> &str {
        self.as_ref()
    }
}

impl AsRef<str> for InlinableString {
    fn as_ref(&self) -> &str {
        match *self {
            InlinableString::Heap(ref s) => s.as_ref(),
            InlinableString::Inline(ref s) => s.as_ref(),
        }
    }
}

impl<'a> From<&'a str> for InlinableString {
    fn from(string: &'a str) -> InlinableString {
        let string_len = string.len();
        if string_len <= INLINE_STRING_CAPACITY {
            InlinableString::Inline(InlineString::from(string))
        } else {
            InlinableString::Heap(String::from(string))
        }
    }
}

impl Default for InlinableString {
    fn default() -> Self {
        InlinableString::new()
    }
}

impl fmt::Display for InlinableString {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            InlinableString::Heap(ref s) => s.fmt(f),
            InlinableString::Inline(ref s) => s.fmt(f),
        }
    }
}

impl ops::Index<ops::Range<usize>> for InlinableString {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::Range<usize>) -> &str {
        match *self {
            InlinableString::Heap(ref s) => s.index(index),
            InlinableString::Inline(ref s) => s.index(index),
        }
    }
}

impl ops::Index<ops::RangeTo<usize>> for InlinableString {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeTo<usize>) -> &str {
        match *self {
            InlinableString::Heap(ref s) => s.index(index),
            InlinableString::Inline(ref s) => s.index(index),
        }
    }
}

impl ops::Index<ops::RangeFrom<usize>> for InlinableString {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeFrom<usize>) -> &str {
        match *self {
            InlinableString::Heap(ref s) => s.index(index),
            InlinableString::Inline(ref s) => s.index(index),
        }
    }
}

impl ops::Index<ops::RangeFull> for InlinableString {
    type Output = str;

    #[inline]
    fn index(&self, index: ops::RangeFull) -> &str {
        match *self {
            InlinableString::Heap(ref s) => s.index(index),
            InlinableString::Inline(ref s) => s.index(index),
        }
    }
}

impl ops::Deref for InlinableString {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        match *self {
            InlinableString::Heap(ref s) => s.deref(),
            InlinableString::Inline(ref s) => s.deref(),
        }
    }
}

impl PartialEq<InlinableString> for InlinableString {
    #[inline]
    fn eq(&self, rhs: &InlinableString) -> bool {
        PartialEq::eq(&self[..], &rhs[..])
    }

    #[inline]
    fn ne(&self, rhs: &InlinableString) -> bool {
        PartialEq::ne(&self[..], &rhs[..])
    }
}

macro_rules! impl_eq {
    ($lhs:ty, $rhs: ty) => {
        impl<'a> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool { PartialEq::eq(&self[..], &other[..]) }
            #[inline]
            fn ne(&self, other: &$rhs) -> bool { PartialEq::ne(&self[..], &other[..]) }
        }

        impl<'a> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool { PartialEq::eq(&self[..], &other[..]) }
            #[inline]
            fn ne(&self, other: &$lhs) -> bool { PartialEq::ne(&self[..], &other[..]) }
        }

    }
}

impl_eq! { InlinableString, str }
impl_eq! { InlinableString, String }
impl_eq! { InlinableString, &'a str }
impl_eq! { InlinableString, InlineString }
impl_eq! { Cow<'a, str>, InlinableString }

impl<'a> StringExt<'a> for InlinableString {
    #[inline]
    fn new() -> Self {
        InlinableString::Inline(InlineString::new())
    }

    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        if capacity <= INLINE_STRING_CAPACITY {
            InlinableString::Inline(InlineString::new())
        } else {
            InlinableString::Heap(String::with_capacity(capacity))
        }
    }

    #[inline]
    fn from_utf8(vec: Vec<u8>) -> Result<Self, FromUtf8Error> {
        String::from_utf8(vec).map(InlinableString::Heap)
    }

    #[inline]
    fn from_utf16(v: &[u16]) -> Result<Self, FromUtf16Error> {
        String::from_utf16(v).map(InlinableString::Heap)
    }

    #[inline]
    fn from_utf16_lossy(v: &[u16]) -> Self {
        InlinableString::Heap(String::from_utf16_lossy(v))
    }

    #[inline]
    unsafe fn from_raw_parts(buf: *mut u8, length: usize, capacity: usize) -> Self {
        InlinableString::Heap(String::from_raw_parts(buf, length, capacity))
    }

    #[inline]
    unsafe fn from_utf8_unchecked(bytes: Vec<u8>) -> Self {
        InlinableString::Heap(String::from_utf8_unchecked(bytes))
    }

    #[inline]
    fn into_bytes(self) -> Vec<u8> {
        match self {
            InlinableString::Heap(s) => s.into_bytes(),
            InlinableString::Inline(s) => Vec::from(&s[..]),
        }
    }

    #[inline]
    fn push_str(&mut self, string: &str) {
        let promoted = match *self {
            InlinableString::Heap(ref mut s) => {
                s.push_str(string);
                return;
            },
            InlinableString::Inline(ref mut s) => {
                if s.push_str(string).is_ok() {
                    return;
                }
                let mut s = String::from(s.as_ref());
                s.push_str(string);
                s
            }
        };
        mem::swap(self, &mut InlinableString::Heap(promoted));
    }

    #[inline]
    fn capacity(&self) -> usize {
        match *self {
            InlinableString::Heap(ref s) => s.capacity(),
            InlinableString::Inline(_) => INLINE_STRING_CAPACITY,
        }
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        let promoted = match *self {
            InlinableString::Heap(ref mut s) => {
                s.reserve(additional);
                return;
            },
            InlinableString::Inline(ref s) => {
                let new_capacity = s.len() + additional;
                if new_capacity <= INLINE_STRING_CAPACITY {
                    return;
                }
                let mut promoted = String::with_capacity(new_capacity);
                promoted.push_str(&s);
                promoted
            }
        };
        mem::swap(self, &mut InlinableString::Heap(promoted));
    }

    #[inline]
    fn reserve_exact(&mut self, additional: usize) {
        let promoted = match *self {
            InlinableString::Heap(ref mut s) => {
                s.reserve_exact(additional);
                return;
            },
            InlinableString::Inline(ref s) => {
                let new_capacity = s.len() + additional;
                if new_capacity <= INLINE_STRING_CAPACITY {
                    return;
                }
                let mut promoted = String::with_capacity(new_capacity);
                promoted.push_str(&s);
                promoted
            }
        };
        mem::swap(self, &mut InlinableString::Heap(promoted));
    }

    #[inline]
    fn shrink_to_fit(&mut self) {
        if self.len() <= INLINE_STRING_CAPACITY {
            let demoted = if let InlinableString::Heap(ref s) = *self {
                InlineString::from(s.as_ref())
            } else {
                return;
            };
            mem::swap(self, &mut InlinableString::Inline(demoted));
            return;
        }

        match *self {
            InlinableString::Heap(ref mut s) => s.shrink_to_fit(),
            _ => panic!("inlinable_string: internal error: this branch should be unreachable"),
        };
    }

    #[inline]
    fn push(&mut self, ch: char) {
        let promoted = match *self {
            InlinableString::Heap(ref mut s) => {
                s.push(ch);
                return;
            },
            InlinableString::Inline(ref mut s) => {
                if s.push(ch).is_ok() {
                    return;
                }

                let mut promoted = String::with_capacity(s.len() + 1);
                promoted.push_str(s.as_ref());
                promoted.push(ch);
                promoted
            },
        };

        mem::swap(self, &mut InlinableString::Heap(promoted));
    }

    #[inline]
    fn as_bytes(&self) -> &[u8] {
        match *self {
            InlinableString::Heap(ref s) => s.as_bytes(),
            InlinableString::Inline(ref s) => s.as_bytes(),
        }
    }

    #[inline]
    fn truncate(&mut self, new_len: usize) {
        match *self {
            InlinableString::Heap(ref mut s) => s.truncate(new_len),
            InlinableString::Inline(ref mut s) => s.truncate(new_len),
        };
    }

    #[inline]
    fn pop(&mut self) -> Option<char> {
        match *self {
            InlinableString::Heap(ref mut s) => s.pop(),
            InlinableString::Inline(ref mut s) => s.pop(),
        }
    }

    #[inline]
    fn remove(&mut self, idx: usize) -> char {
        match *self {
            InlinableString::Heap(ref mut s) => s.remove(idx),
            InlinableString::Inline(ref mut s) => s.remove(idx),
        }
    }

    #[inline]
    fn insert(&mut self, idx: usize, ch: char) {
        let promoted = match *self {
            InlinableString::Heap(ref mut s) => {
                s.insert(idx, ch);
                return;
            },
            InlinableString::Inline(ref mut s) => {
                if s.insert(idx, ch).is_ok() {
                    return;
                }

                let mut promoted = String::with_capacity(s.len() + 1);
                promoted.push_str(&s[..idx]);
                promoted.push(ch);
                promoted.push_str(&s[idx..]);
                promoted
            },
        };

        mem::swap(self, &mut InlinableString::Heap(promoted));
    }

    #[inline]
    unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        match *self {
            InlinableString::Heap(ref mut s) => &mut s.as_mut_vec()[..],
            InlinableString::Inline(ref mut s) => s.as_mut_slice(),
        }
    }

    #[inline]
    fn len(&self) -> usize {
        match *self {
            InlinableString::Heap(ref s) => s.len(),
            InlinableString::Inline(ref s) => s.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{InlinableString, StringExt, INLINE_STRING_CAPACITY};
    use std::iter::FromIterator;

    // First, specifically test operations that overflow InlineString's capacity
    // and require promoting the string to heap allocation.

    #[test]
    fn test_push_str() {
        let mut s = InlinableString::new();
        s.push_str("small");
        assert_eq!(s, "small");

        let long_str = "this is a really long string that is much larger than
                        INLINE_STRING_CAPACITY and so cannot be stored inline.";
        s.push_str(long_str);
        assert_eq!(s, String::from("small") + long_str);
    }

    #[test]
    fn test_push() {
        let mut s = InlinableString::new();

        for _ in 0..INLINE_STRING_CAPACITY {
            s.push('a');
        }
        s.push('a');

        assert_eq!(s, String::from_iter((0..INLINE_STRING_CAPACITY + 1).map(|_| 'a')));
    }

    #[test]
    fn test_insert() {
        let mut s = InlinableString::new();

        for _ in 0..INLINE_STRING_CAPACITY {
            s.insert(0, 'a');
        }
        s.insert(0, 'a');

        assert_eq!(s, String::from_iter((0..INLINE_STRING_CAPACITY + 1).map(|_| 'a')));
    }

    // Next, some general sanity tests.

    #[test]
    fn test_new() {
        let s = <InlinableString as StringExt>::new();
        assert!(StringExt::is_empty(&s));
    }

    #[test]
    fn test_with_capacity() {
        let s = <InlinableString as StringExt>::with_capacity(10);
        assert!(StringExt::capacity(&s) >= 10);
    }

    #[test]
    fn test_from_utf8() {
        let s = <InlinableString as StringExt>::from_utf8(vec![104, 101, 108, 108, 111]);
        assert_eq!(s.unwrap(), "hello");
    }

    #[test]
    fn test_from_utf16() {
        let v = &mut [0xD834, 0xDD1E, 0x006d, 0x0075,
                      0x0073, 0x0069, 0x0063];
        let s = <InlinableString as StringExt>::from_utf16(v);
        assert_eq!(s.unwrap(), "𝄞music");
    }

    #[test]
    fn test_from_utf16_lossy() {
        let input = b"Hello \xF0\x90\x80World";
        let output = <InlinableString as StringExt>::from_utf8_lossy(input);
        assert_eq!(output, "Hello \u{FFFD}World");
    }

    #[test]
    fn test_into_bytes() {
        let s = InlinableString::from("hello");
        let bytes = StringExt::into_bytes(s);
        assert_eq!(bytes, [104, 101, 108, 108, 111]);
    }

    #[test]
    fn test_capacity() {
        let s = <InlinableString as StringExt>::with_capacity(100);
        assert!(InlinableString::capacity(&s) >= 100);
    }

    #[test]
    fn test_reserve() {
        let mut s = <InlinableString as StringExt>::new();
        StringExt::reserve(&mut s, 100);
        assert!(InlinableString::capacity(&s) >= 100);
    }

    #[test]
    fn test_reserve_exact() {
        let mut s = <InlinableString as StringExt>::new();
        StringExt::reserve_exact(&mut s, 100);
        assert!(InlinableString::capacity(&s) >= 100);
    }

    #[test]
    fn test_shrink_to_fit() {
        let mut s = <InlinableString as StringExt>::with_capacity(100);
        StringExt::push_str(&mut s, "foo");
        StringExt::shrink_to_fit(&mut s);
        assert_eq!(InlinableString::capacity(&s), INLINE_STRING_CAPACITY);
    }

    #[test]
    fn test_truncate() {
        let mut s = InlinableString::from("foo");
        StringExt::truncate(&mut s, 1);
        assert_eq!(s, "f");
    }

    #[test]
    fn test_pop() {
        let mut s = InlinableString::from("foo");
        assert_eq!(StringExt::pop(&mut s), Some('o'));
        assert_eq!(StringExt::pop(&mut s), Some('o'));
        assert_eq!(StringExt::pop(&mut s), Some('f'));
        assert_eq!(StringExt::pop(&mut s), None);
    }
}

#[cfg(test)]
#[cfg(feature = "nightly")]
mod benches {
    use test::Bencher;

    #[bench]
    fn its_fast(b: &mut Bencher) {
    }
}