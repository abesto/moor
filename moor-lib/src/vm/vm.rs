use std::sync::Arc;

use anyhow::bail;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::trace;

use crate::compiler::builtins::BUILTINS;
use crate::compiler::labels::Label;
use crate::db::state::WorldState;
use crate::model::objects::ObjFlag;
use crate::model::verbs::VerbInfo;
use crate::model::ObjectError::{PropertyNotFound, PropertyPermissionDenied, VerbNotFound};
use crate::tasks::command_parse::ParsedCommand;
use crate::tasks::scheduler::TaskId;
use crate::tasks::Sessions;
use crate::util::bitenum::BitEnum;
use crate::var::error::Error::{E_INVIND, E_PERM, E_PROPNF, E_TYPE, E_VERBNF};
use crate::var::{v_objid, v_str, v_string, Objid, Var, Variant, NOTHING};
use crate::vm::activation::{Activation, Caller};
use crate::vm::bf_server::BfNoop;
use crate::vm::opcode::Op;
use crate::vm::vm_unwind::FinallyReason;

#[async_trait]
pub(crate) trait BfFunction: Sync + Send {
    fn name(&self) -> &str;
    async fn call(
        &self,
        world_state: &mut dyn WorldState,
        frame: &mut Activation,
        sessions: Arc<RwLock<dyn Sessions>>,
        args: &[Var],
    ) -> Result<Var, anyhow::Error>;
}

pub struct VM {
    // Activation stack.
    pub(crate) stack: Vec<Activation>,
    pub(crate) bf_funcs: Vec<Arc<Box<dyn BfFunction>>>,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum ExecutionResult {
    Complete(Var),
    More,
    Exception(FinallyReason),
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

impl VM {
    pub fn new() -> Self {
        let mut bf_funcs: Vec<Arc<Box<dyn BfFunction>>> = Vec::with_capacity(BUILTINS.len());
        for _ in 0..BUILTINS.len() {
            bf_funcs.push(Arc::new(Box::new(BfNoop {})))
        }
        let _bf_noop = Box::new(BfNoop {});
        let mut vm = Self {
            stack: vec![],
            bf_funcs,
        };

        vm.register_bf_server().unwrap();
        vm.register_bf_num().unwrap();
        vm.register_bf_values().unwrap();
        vm.register_bf_strings().unwrap();
        vm.register_bf_list_sets().unwrap();
        vm.register_bf_objects().unwrap();
        vm.register_bf_verbs().unwrap();

        vm
    }

    /// Entry point from scheduler for setting up a command execution in this VM.
    pub fn setup_verb_command(
        &mut self,
        task_id: TaskId,
        vi: VerbInfo,
        obj: Objid,
        this: Objid,
        player: Objid,
        player_flags: BitEnum<ObjFlag>,
        command: &ParsedCommand,
    ) -> Result<(), anyhow::Error> {
        let Some(binary) = vi.attrs.program.clone() else {
            bail!(VerbNotFound(obj, command.verb.to_string()))
        };

        let mut a = Activation::new_for_method(
            task_id,
            binary,
            NOTHING,
            this,
            player,
            player_flags,
            vi,
            &command.args,
            vec![],
        )?;

        // TODO use pre-set constant offsets for these like LambdaMOO does.
        a.set_var("argstr", v_string(command.argstr.clone()))
            .unwrap();
        a.set_var("dobj", v_objid(command.dobj)).unwrap();
        a.set_var("dobjstr", v_string(command.dobjstr.clone()))
            .unwrap();
        a.set_var("prepstr", v_string(command.prepstr.clone()))
            .unwrap();
        a.set_var("iobj", v_objid(command.iobj)).unwrap();
        a.set_var("iobjstr", v_string(command.iobjstr.clone()))
            .unwrap();

        self.stack.push(a);
        trace!(
            "do_method_cmd: {}:{}({:?}); argstr: {}",
            this,
            command.verb,
            command.args,
            command.argstr
        );

        Ok(())
    }

    /// Entry point from scheduler for setting up a method execution (non-command) in this VM.
    pub fn setup_verb_method_call(
        &mut self,
        task_id: TaskId,
        state: &mut dyn WorldState,
        obj: Objid,
        verb_name: &str,
        this: Objid,
        player: Objid,
        player_flags: BitEnum<ObjFlag>,
        args: &[Var],
    ) -> Result<(), anyhow::Error> {
        let vi = state.find_method_verb_on(obj, verb_name)?;

        let Some(binary) = vi.attrs.program.clone() else {
            bail!(VerbNotFound(obj, verb_name.to_string()))
        };

        let mut a = Activation::new_for_method(
            task_id,
            binary,
            NOTHING,
            this,
            player,
            player_flags,
            vi,
            args,
            vec![],
        )?;

        a.set_var("argstr", v_str("")).unwrap();
        a.set_var("dobj", v_objid(NOTHING)).unwrap();
        a.set_var("dobjstr", v_str("")).unwrap();
        a.set_var("prepstr", v_str("")).unwrap();
        a.set_var("iobj", v_objid(NOTHING)).unwrap();
        a.set_var("iobjstr", v_str("")).unwrap();

        self.stack.push(a);

        trace!("do_method_verb: {}:{}({:?})", this, verb_name, args);

        Ok(())
    }

    /// Entry point for VM setting up a method call from the Op::CallVerb instruction.
    pub(crate) fn call_verb(
        &mut self,
        state: &mut dyn WorldState,
        this: Objid,
        verb: String,
        args: &[Var],
    ) -> Result<ExecutionResult, anyhow::Error> {
        let self_valid = state.valid(this)?;
        if !self_valid {
            return self.push_error(E_INVIND);
        }
        // find callable verb
        let Ok(verbinfo) = state.find_method_verb_on(this, verb.as_str()) else {
            return self.push_error_msg(E_VERBNF, format!("Verb \"{}\" not found", verb));
        };
        let Some(binary) = verbinfo.attrs.program.clone() else {
            return self.push_error_msg(
                E_VERBNF,
                format!("Verb \"{}\" is not a program", verb),
            );
        };

        let caller = self.top().this;

        let top = self.top();
        let mut callers = top.callers.to_vec();
        let task_id = top.task_id;

        callers.push(Caller {
            this,
            verb_name: top.verb_name().to_string(),
            programmer: top.verb_owner(),
            verb_loc: top.verb_definer(),
            player: top.player,
            line_number: 0,
        });

        let mut a = Activation::new_for_method(
            task_id,
            binary,
            caller,
            this,
            top.player,
            top.player_flags,
            verbinfo,
            args,
            callers,
        )?;

        // TODO use pre-set constant offsets for these like LambdaMOO does.
        let argstr = self.top().get_var("argstr");
        let dobj = self.top().get_var("dobj");
        let dobjstr = self.top().get_var("dobjstr");
        let prepstr = self.top().get_var("prepstr");
        let iobj = self.top().get_var("iobj");
        let iobjstr = self.top().get_var("iobjstr");

        a.set_var("argstr", argstr.unwrap()).unwrap();
        a.set_var("dobj", dobj.unwrap()).unwrap();
        a.set_var("dobjstr", dobjstr.unwrap()).unwrap();
        a.set_var("prepstr", prepstr.unwrap()).unwrap();
        a.set_var("iobj", iobj.unwrap()).unwrap();
        a.set_var("iobjstr", iobjstr.unwrap()).unwrap();

        self.stack.push(a);
        trace!("call_verb: {}:{}({:?})", this, verb, args);
        Ok(ExecutionResult::More)
    }

    pub(crate) fn top_mut(&mut self) -> &mut Activation {
        self.stack.last_mut().expect("activation stack underflow")
    }

    pub(crate) fn top(&self) -> &Activation {
        self.stack.last().expect("activation stack underflow")
    }

    pub(crate) fn pop(&mut self) -> Var {
        self.top_mut()
            .pop()
            .unwrap_or_else(|| panic!("stack underflow, activation depth: {}", self.stack.len()))
    }

    pub(crate) fn push(&mut self, v: &Var) {
        self.top_mut().push(v.clone())
    }

    pub(crate) fn next_op(&mut self) -> Option<Op> {
        self.top_mut().next_op()
    }

    pub(crate) fn jump(&mut self, label: Label) {
        self.top_mut().jump(label)
    }

    pub(crate) fn get_env(&mut self, id: Label) -> Var {
        self.top().environment[id.0 as usize].clone()
    }

    pub(crate) fn set_env(&mut self, id: Label, v: &Var) {
        self.top_mut().environment[id.0 as usize] = v.clone();
    }

    pub(crate) fn peek(&self, amt: usize) -> Vec<Var> {
        self.top().peek(amt)
    }

    pub(crate) fn peek_top(&self) -> Var {
        self.top().peek_top().expect("stack underflow")
    }

    pub(crate) fn get_prop(
        &mut self,
        state: &mut dyn WorldState,
        player_flags: BitEnum<ObjFlag>,
        propname: Var,
        obj: Var,
    ) -> Result<ExecutionResult, anyhow::Error> {
        let Variant::Str(propname) = propname.variant() else {
            return self.push_error(E_TYPE);
        };

        let Variant::Obj(obj) = obj.variant() else {
            return self.push_error(E_INVIND);
        };

        let result = state.retrieve_property(*obj, propname.as_str(), player_flags);
        let v = match result {
            Ok(v) => v,
            Err(e) => match e {
                PropertyPermissionDenied(_, _) => return self.push_error(E_PERM),
                PropertyNotFound(_, _) => return self.push_error(E_PROPNF),
                _ => {
                    panic!("Unexpected error in property retrieval: {:?}", e);
                }
            },
        };
        self.push(&v);
        Ok(ExecutionResult::More)
    }
}