// Copyright (C) 2024 Ryan Daum <ryan.daum@gmail.com>
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, version 3.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// this program. If not, see <https://www.gnu.org/licenses/>.
//

use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::ops::Range;
use std::str;
use std::str::FromStr;
use str::from_utf8;

use bincode::de::{BorrowDecoder, Decoder};
use bincode::enc::Encoder;
use bincode::error::{DecodeError, EncodeError};
use bincode::{BorrowDecode, Decode, Encode};
use bytes::Bytes;

use crate::var::error::Error;
use crate::var::{v_err, v_str, v_string, Var};
use crate::{AsByteBuffer, DecodingError, EncodingError};

#[derive(Clone, Debug)]
pub struct Str(Bytes);

impl Str {
    #[must_use]
    pub fn from_string(s: String) -> Self {
        let s = s.into_bytes();
        let sr = Bytes::from(s);
        Self(sr)
    }

    pub fn get(&self, offset: usize) -> Option<Var> {
        let s = from_utf8(&self.0).unwrap();
        let r = s.get(offset..offset + 1);
        r.map(v_str)
    }

    #[must_use]
    pub fn set(&self, offset: usize, r: &Self) -> Var {
        if r.len() != 1 {
            return v_err(Error::E_RANGE);
        }
        if offset >= self.0.len() {
            return v_err(Error::E_RANGE);
        }
        let mut s = from_utf8(&self.0).unwrap().to_string();
        s.replace_range(offset..=offset, r.as_str());
        v_string(s)
    }

    pub fn get_range(&self, range: Range<usize>) -> Option<Var> {
        let s = from_utf8(&self.0).unwrap();
        let r = s.get(range);
        r.map(v_str)
    }

    #[must_use]
    pub fn append(&self, other: &Self) -> Var {
        v_string(format!(
            "{}{}",
            from_utf8(&self.0).unwrap(),
            from_utf8(&other.0).unwrap()
        ))
    }

    #[must_use]
    pub fn append_str(&self, other: &str) -> Var {
        v_string(format!("{}{}", from_utf8(&self.0).unwrap(), other))
    }

    #[must_use]
    pub fn append_string(&self, other: String) -> Var {
        v_string(format!("{}{}", from_utf8(&self.0).unwrap(), other))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        from_utf8(&self.0).unwrap().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        from_utf8(&self.0).unwrap().is_empty()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        from_utf8(&self.0).unwrap()
    }

    #[must_use]
    pub fn substring(&self, range: Range<usize>) -> Self {
        let s = from_utf8(&self.0).unwrap();
        let s = s.get(range).unwrap_or("");
        Self::from_string(s.to_string())
    }
}

// MOO's string comparisons are all case-insensitive. To get case-sensitive you have to use
// bf_is_member and bf_strcmp.
impl PartialEq for Str {
    fn eq(&self, other: &Self) -> bool {
        let s = from_utf8(&self.0).unwrap();
        let o = from_utf8(&other.0).unwrap();
        s.eq_ignore_ascii_case(o)
    }
}
impl Eq for Str {}

impl Hash for Str {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let s = from_utf8(&self.0).unwrap();
        s.to_lowercase().hash(state)
    }
}

impl FromStr for Str {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_string();
        Ok(Self::from_string(s))
    }
}

impl Display for Str {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", from_utf8(&self.0).unwrap()))
    }
}

impl Encode for Str {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        let str = self.as_str().to_string();
        str.encode(encoder)
    }
}

impl Decode for Str {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let str = String::decode(decoder)?;
        Ok(Self::from_string(str))
    }
}

impl<'de> BorrowDecode<'de> for Str {
    fn borrow_decode<D: BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let str = String::borrow_decode(decoder)?;
        Ok(Self::from_string(str))
    }
}

impl Ord for Str {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let s = from_utf8(&self.0).unwrap();
        let o = from_utf8(&other.0).unwrap();
        s.to_lowercase().cmp(&o.to_lowercase())
    }
}

impl PartialOrd for Str {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl AsByteBuffer for Str {
    fn size_bytes(&self) -> usize {
        self.0.len()
    }

    fn with_byte_buffer<R, F: FnMut(&[u8]) -> R>(&self, mut f: F) -> Result<R, EncodingError> {
        Ok(f(self.0.as_ref()))
    }

    fn make_copy_as_vec(&self) -> Result<Vec<u8>, EncodingError> {
        Ok(self.0.to_vec())
    }

    fn from_bytes(bytes: Bytes) -> Result<Self, DecodingError>
    where
        Self: Sized,
    {
        Ok(Self(bytes))
    }

    fn as_bytes(&self) -> Result<Bytes, EncodingError> {
        Ok(self.0.clone())
    }
}
