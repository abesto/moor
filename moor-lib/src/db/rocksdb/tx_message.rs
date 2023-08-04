use tokio::sync::oneshot::Sender;

use moor_value::util::bitenum::BitEnum;
use moor_value::var::objid::Objid;
use moor_value::var::Var;

use crate::db::rocksdb::tx_server::{PropHandle, VerbHandle};
use crate::db::CommitResult;
use crate::model::objects::{ObjAttrs, ObjFlag};
use crate::model::props::PropFlag;
use crate::model::r#match::VerbArgsSpec;
use crate::model::verbs::VerbFlag;
use crate::model::ObjectError;
use crate::vm::opcode::Binary;

#[allow(dead_code)] // TODO Not all of these are used yet, but they will be. For now shut up the compiler.
pub(crate) enum Message {
    // Objects
    CreateObject(Option<Objid>, ObjAttrs, Sender<Result<Objid, ObjectError>>),
    GetLocationOf(Objid, Sender<Result<Objid, ObjectError>>),
    GetContentsOf(Objid, Sender<Result<Vec<Objid>, ObjectError>>),
    SetLocation(Objid, Objid, Sender<Result<(), ObjectError>>),
    GetFlagsOf(Objid, Sender<Result<BitEnum<ObjFlag>, ObjectError>>),
    SetFlags(Objid, BitEnum<ObjFlag>, Sender<Result<(), ObjectError>>),
    GetObjectName(Objid, Sender<Result<String, ObjectError>>),
    SetObjectName(Objid, String, Sender<Result<(), ObjectError>>),
    GetParentOf(Objid, Sender<Result<Objid, ObjectError>>),
    SetParent(Objid, Objid, Sender<Result<(), ObjectError>>),
    GetChildrenOf(Objid, Sender<Result<Vec<Objid>, ObjectError>>),
    GetObjectOwner(Objid, Sender<Result<Objid, ObjectError>>),
    SetObjectOwner(Objid, Objid, Sender<Result<(), ObjectError>>),

    // Verbs
    // Get information about all verbs declared on a given object
    GetVerbs(Objid, Sender<Result<Vec<VerbHandle>, ObjectError>>),
    // Get information about a specific verb on a given object by its unique id
    GetVerb(Objid, u128, Sender<Result<VerbHandle, ObjectError>>),
    // Get information about a specific verb on a given object by one of its names
    GetVerbByName(Objid, String, Sender<Result<VerbHandle, ObjectError>>),
    // Get information about a specific verb on a given object by its index in the list of verbs
    GetVerbByIndex(Objid, usize, Sender<Result<VerbHandle, ObjectError>>),
    // Get the program for a specific verb on a given object by its unique id
    GetProgram(Objid, u128, Sender<Result<Binary, ObjectError>>),
    // Search the inheritance hierarchy of an object to find a verb by name & argspec
    // (If argspec is not specified, then "this none this" is assumed.)
    ResolveVerb(
        Objid,
        String,
        Option<VerbArgsSpec>,
        Sender<Result<VerbHandle, ObjectError>>,
    ),
    // Update (non-program) data about a verb.
    SetVerbInfo {
        obj: Objid,
        uuid: u128,
        owner: Option<Objid>,
        names: Option<Vec<String>>,
        flags: Option<BitEnum<VerbFlag>>,
        args: Option<VerbArgsSpec>,
        reply: Sender<Result<(), ObjectError>>,
    },

    // Add a verb on an object
    AddVerb {
        location: Objid,
        owner: Objid,
        names: Vec<String>,
        program: Binary,
        flags: BitEnum<VerbFlag>,
        args: VerbArgsSpec,
        reply: Sender<Result<(), ObjectError>>,
    },
    // Delete a verb from an object
    DeleteVerb {
        location: Objid,
        uuid: u128,
        reply: Sender<Result<(), ObjectError>>,
    },
    RetrieveVerb(
        Objid,
        String,
        Sender<Result<(Binary, VerbHandle), ObjectError>>,
    ),

    // Properties

    // Retrieve the list of properties defined on this object.
    GetProperties(Objid, Sender<Result<Vec<PropHandle>, ObjectError>>),
    // Retrieve a specific property by its unique id.
    RetrieveProperty(Objid, u128, Sender<Result<Var, ObjectError>>),
    // Set a property's value by its id.
    SetProperty(Objid, u128, Var, Sender<Result<(), ObjectError>>),
    DefineProperty {
        definer: Objid,
        obj: Objid,
        name: String,
        owner: Objid,
        perms: BitEnum<PropFlag>,
        value: Option<Var>,
        is_clear: bool,
        reply: Sender<Result<PropHandle, ObjectError>>,
    },
    SetPropertyInfo {
        obj: Objid,
        uuid: u128,
        new_owner: Option<Objid>,
        new_perms: Option<BitEnum<PropFlag>>,
        new_name: Option<String>,
        is_clear: Option<bool>,
        reply: Sender<Result<(), ObjectError>>,
    },
    DeleteProperty(Objid, u128, Sender<Result<(), ObjectError>>),
    ResolveProperty(
        Objid,
        String,
        Sender<Result<(PropHandle, Var), ObjectError>>,
    ),
    Valid(Objid, Sender<bool>),
    Commit(Sender<CommitResult>),
    Rollback(Sender<()>),
}
