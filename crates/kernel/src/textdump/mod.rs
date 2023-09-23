/// Representation of the structure of objects verbs etc as read from a LambdaMOO textdump'd db
/// file.
use std::collections::BTreeMap;
use std::io::{BufReader, Read};

use moor_values::var::objid::Objid;
use moor_values::var::Var;

pub mod load_db;
pub mod read;

#[derive(Clone)]
pub struct Verbdef {
    pub name: String,
    pub owner: Objid,
    pub flags: u16,
    pub prep: i16,
}

#[derive(Clone)]
pub struct Propval {
    pub value: Var,
    pub owner: Objid,
    pub flags: u8,
    pub is_clear: bool,
}

pub struct Object {
    pub id: Objid,
    pub owner: Objid,
    pub location: Objid,
    pub contents: Objid,
    pub next: Objid,
    pub parent: Objid,
    pub child: Objid,
    pub sibling: Objid,
    pub name: String,
    pub flags: u8,
    pub verbdefs: Vec<Verbdef>,
    pub propdefs: Vec<String>,
    pub propvals: Vec<Propval>,
}

#[derive(Clone, Debug)]
pub struct Verb {
    pub(crate) objid: Objid,
    pub(crate) verbnum: usize,
    pub(crate) program: String,
}

pub struct TextdumpReader<R: Read> {
    reader: BufReader<R>,
}

impl<R: Read> TextdumpReader<R> {
    pub fn new(reader: BufReader<R>) -> Self {
        Self { reader }
    }
}

pub struct Textdump {
    pub version: String,
    pub objects: BTreeMap<Objid, Object>,
    pub users: Vec<Objid>,
    pub verbs: BTreeMap<(Objid, usize), Verb>,
}