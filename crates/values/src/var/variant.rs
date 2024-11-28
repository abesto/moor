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

use crate::var::list::List;
use crate::var::Associative;
use crate::var::{map, string, Sequence};
use crate::var::{Error, Obj};
use bincode::{Decode, Encode};
use decorum::R64;
use num_traits::ToPrimitive;
use std::cmp::Ordering;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

/// Our series of types
#[derive(Clone, Encode, Decode)]
pub enum Variant {
    None,
    Obj(Obj),
    Int(i64),
    Float(f64),
    List(List),
    Str(string::Str),
    Map(map::Map),
    Err(Error),
}

impl Hash for Variant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Variant::None => 0.hash(state),
            Variant::Obj(o) => o.hash(state),
            Variant::Int(i) => i.hash(state),
            Variant::Float(f) => f.to_f64().unwrap().to_bits().hash(state),
            Variant::List(l) => l.hash(state),
            Variant::Str(s) => s.hash(state),
            Variant::Map(m) => m.hash(state),
            Variant::Err(e) => e.hash(state),
        }
    }
}

impl Ord for Variant {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Variant::None, Variant::None) => Ordering::Equal,
            (Variant::Obj(l), Variant::Obj(r)) => l.cmp(r),
            (Variant::Int(l), Variant::Int(r)) => l.cmp(r),
            (Variant::Float(l), Variant::Float(r)) => {
                // For floats, we wrap in decorum first.
                let l = R64::from(l.to_f64().unwrap());
                let r = R64::from(r.to_f64().unwrap());
                l.cmp(&r)
            }
            (Variant::List(l), Variant::List(r)) => l.cmp(r),
            (Variant::Str(l), Variant::Str(r)) => l.cmp(r),
            (Variant::Map(l), Variant::Map(r)) => l.cmp(r),
            (Variant::Err(l), Variant::Err(r)) => l.cmp(r),
            (Variant::None, _) => Ordering::Less,
            (_, Variant::None) => Ordering::Greater,
            (Variant::Obj(_), _) => Ordering::Less,
            (_, Variant::Obj(_)) => Ordering::Greater,
            (Variant::Int(_), _) => Ordering::Less,
            (_, Variant::Int(_)) => Ordering::Greater,
            (Variant::Float(_), _) => Ordering::Less,
            (_, Variant::Float(_)) => Ordering::Greater,
            (Variant::List(_), _) => Ordering::Less,
            (_, Variant::List(_)) => Ordering::Greater,
            (Variant::Str(_), _) => Ordering::Less,
            (_, Variant::Str(_)) => Ordering::Greater,
            (Variant::Map(_), _) => Ordering::Less,
            (_, Variant::Map(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Variant {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Debug for Variant {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Variant::None => write!(f, "None"),
            Variant::Obj(o) => write!(f, "Object({})", o),
            Variant::Int(i) => write!(f, "Integer({})", i),
            Variant::Float(fl) => write!(f, "Float({})", fl),
            Variant::List(l) => {
                // Items...
                let r = l.iter();
                let i: Vec<_> = r.collect();
                write!(f, "List([size = {}, items = {:?}])", l.len(), i)
            }
            Variant::Str(s) => write!(f, "String({:?})", s.as_string()),
            Variant::Map(m) => {
                // Items...
                let r = m.iter();
                let i: Vec<_> = r.collect();
                write!(f, "Map([size = {}, items = {:?}])", m.len(), i)
            }
            Variant::Err(e) => write!(f, "Error({:?})", e),
        }
    }
}

impl PartialEq<Self> for Variant {
    fn eq(&self, other: &Self) -> bool {
        // If the types are different, they're not equal.
        match (self, other) {
            (Variant::Str(s), Variant::Str(o)) => s == o,
            (Variant::Int(s), Variant::Int(o)) => s == o,
            (Variant::Float(s), Variant::Float(o)) => s == o,
            (Variant::Obj(s), Variant::Obj(o)) => s == o,
            (Variant::List(s), Variant::List(o)) => s == o,
            (Variant::Map(s), Variant::Map(o)) => s == o,
            (Variant::Err(s), Variant::Err(o)) => s == o,
            (Variant::None, Variant::None) => true,
            _ => false,
        }
    }
}

impl Eq for Variant {}
