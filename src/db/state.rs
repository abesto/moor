use crate::db::CommitResult;
use crate::model::objects::ObjFlag;
use crate::model::props::PropFlag;
use crate::model::var::{Objid, Var};
use crate::model::verbs::VerbInfo;
use crate::model::ObjectError;
use crate::server::parse_cmd::ParsedCommand;
use crate::util::bitenum::BitEnum;
use crate::vm::opcode::Binary;

pub trait WorldState: Send + Sync {
    // Get the location of the given object.
    fn location_of(&mut self, obj: Objid) -> Result<Objid, ObjectError>;

    // Get the contents of a given object.
    fn contents_of(&mut self, obj: Objid) -> Result<Vec<Objid>, ObjectError>;

    // Retrieve a verb/method from the given object.
    fn retrieve_verb(&mut self, obj: Objid, vname: &str)
        -> Result<(Binary, VerbInfo), ObjectError>;

    // Retrieve a property from the given object, walking transitively up its inheritance chain.
    fn retrieve_property(
        &mut self,
        obj: Objid,
        pname: &str,
        player_flags: BitEnum<ObjFlag>,
    ) -> Result<Var, ObjectError>;

    // Update a property on the given object.
    fn update_property(
        &mut self,
        obj: Objid,
        pname: &str,
        player_flags: BitEnum<ObjFlag>,
        value: &Var,
    ) -> Result<(), ObjectError>;

    // Add a property for the given object.
    fn add_property(
        &mut self,
        obj: Objid,
        pname: &str,
        owner: Objid,
        prop_flags: BitEnum<PropFlag>,
        initial_value: Option<Var>,
    ) -> Result<(), ObjectError>;

    fn find_command_verb_on(
        &mut self,
        oid: Objid,
        pc: &ParsedCommand,
    ) -> Result<Option<VerbInfo>, ObjectError>;

    // Get the object that is the parent of the given object.
    fn parent_of(&mut self, obj: Objid) -> Result<Objid, ObjectError>;

    // Check the validity of an object.
    fn valid(&mut self, obj: Objid) -> Result<bool, ObjectError>;

    // Get the name & aliases of an object.
    fn names_of(&mut self, obj: Objid) -> Result<(String, Vec<String>), ObjectError>;

    // Commit all modifications made to the state of this world since the start of its transaction.
    fn commit(&mut self) -> Result<CommitResult, anyhow::Error>;

    // Rollback all modifications made to the state of this world since the start of its transaction.
    fn rollback(&mut self) -> Result<(), anyhow::Error>;
}

pub trait WorldStateSource {
    fn new_world_state(&mut self) -> Result<Box<dyn WorldState>, anyhow::Error>;
}
