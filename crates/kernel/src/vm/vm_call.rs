use anyhow::Context;
use tracing::{debug, span, trace, Level};

use moor_values::model::world_state::WorldState;
use moor_values::model::WorldStateError;
use moor_values::var::error::Error::{E_INVIND, E_PERM, E_VARNF, E_VERBNF};
use moor_values::var::objid::Objid;
use moor_values::var::{v_int, Var};

use crate::compiler::builtins::BUILTIN_DESCRIPTORS;
use crate::tasks::{TaskId, VerbCall};
use crate::vm::activation::Activation;
use crate::vm::bf_server::BF_SERVER_EVAL_TRAMPOLINE_RESUME;
use crate::vm::builtin::{BfCallState, BfRet};
use crate::vm::opcode::Program;
use crate::vm::vm_execute::VmExecParams;
use crate::vm::vm_unwind::FinallyReason;
use crate::vm::{ExecutionResult, ForkRequest, VerbExecutionRequest, VM};

pub(crate) fn args_literal(args: &[Var]) -> String {
    args.iter()
        .map(|v| v.to_literal())
        .collect::<Vec<String>>()
        .join(", ")
}

impl VM {
    /// Entry point for preparing a verb call for execution, invoked from the CallVerb opcode
    /// Seek the verb and prepare the call parameters.
    /// All parameters for player, caller, etc. are pulled off the stack.
    /// The call params will be returned back to the task in the scheduler, which will then dispatch
    /// back through to `do_method_call`
    pub(crate) async fn prepare_call_verb(
        &mut self,
        state: &mut dyn WorldState,
        this: Objid,
        verb_name: &str,
        args: &[Var],
    ) -> Result<ExecutionResult, anyhow::Error> {
        let call = VerbCall {
            verb_name: verb_name.to_string(),
            location: this,
            this,
            player: self.top().player,
            args: args.to_vec(),
            // caller her is current-activation 'this', not activation caller() ...
            // unless we're a builtin, in which case we're #-1.
            argstr: "".to_string(),
            caller: self.caller(),
        };
        debug!(
            line = self.top().find_line_no(self.top().pc).unwrap_or(0),
            caller_perms = ?self.top().permissions,
            caller = ?self.caller(),
            this = ?this,
            player = ?self.top().player,
            "Verb call: {}:{}({})",
            this,
            verb_name,
            args_literal(args),
        );

        let self_valid = state.valid(this).await?;
        if !self_valid {
            return self.push_error(E_INVIND);
        }
        // Find the callable verb ...
        let verb_info = match state
            .find_method_verb_on(self.top().permissions, this, verb_name)
            .await
        {
            Ok(vi) => vi,
            Err(WorldStateError::ObjectPermissionDenied) => {
                return self.push_error(E_PERM);
            }
            Err(WorldStateError::VerbPermissionDenied) => {
                return self.push_error(E_PERM);
            }
            Err(WorldStateError::VerbNotFound(_, _)) => {
                return self.push_error_msg(E_VERBNF, format!("Verb \"{}\" not found", verb_name));
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("Error finding verb \"{}\" on object {}", verb_name, this)
                })?;
            }
        };

        // Permissions for the activation are the verb's owner.
        let permissions = verb_info.verbdef().owner();

        Ok(ExecutionResult::ContinueVerb {
            permissions,
            resolved_verb: verb_info,
            call,
            command: self.top().command.clone(),
            trampoline: None,
            trampoline_arg: None,
        })
    }

    /// Setup the VM to execute the verb of the same current name, but using the parent's
    /// version.
    /// TODO this should be done up in task.rs instead. let's add a new ExecutionResult for it.
    pub(crate) async fn prepare_pass_verb(
        &mut self,
        state: &mut dyn WorldState,
        args: &[Var],
    ) -> Result<ExecutionResult, anyhow::Error> {
        // get parent of verb definer object & current verb name.
        let definer = self.top().verb_definer();
        let permissions = self.top().permissions;
        let parent = state.parent_of(permissions, definer).await?;
        let verb = self.top().verb_name.to_string();

        // call verb on parent, but with our current 'this'
        trace!(task_id = self.top().task_id, verb, ?definer, ?parent);

        let Ok(vi) = state
            .find_method_verb_on(permissions, parent, verb.as_str())
            .await
        else {
            return self.raise_error(E_VERBNF);
        };

        let caller = self.caller();
        let call = VerbCall {
            verb_name: verb,
            location: parent,
            this: self.top().this,
            player: self.top().player,
            args: args.to_vec(),
            argstr: "".to_string(),
            caller,
        };

        Ok(ExecutionResult::ContinueVerb {
            permissions,
            resolved_verb: vi,
            call,
            command: self.top().command.clone(),
            trampoline: None,
            trampoline_arg: None,
        })
    }

    /// Entry point from scheduler for actually beginning the dispatch of a method execution
    /// (non-command) in this VM.
    /// Actually creates the activation record and puts it on the stack.
    pub async fn exec_call_request(
        &mut self,
        task_id: TaskId,
        call_request: VerbExecutionRequest,
    ) -> Result<(), anyhow::Error> {
        let span = span!(Level::TRACE, "VC", task_id, ?call_request);
        let span_id = span.id();

        debug!(
            caller = ?call_request.call.caller,
            this = ?call_request.call.this,
            player = ?call_request.call.player,
            "Verb call: {}:{}({})",
            call_request.call.this,
            call_request.call.verb_name,
            args_literal(call_request.call.args.as_slice()),
        );

        let a = Activation::for_call(task_id, call_request, span_id.clone())?;

        self.stack.push(a);

        tracing_enter_span(&span_id, &None);

        Ok(())
    }

    pub async fn exec_eval_request(
        &mut self,
        task_id: TaskId,
        permissions: Objid,
        player: Objid,
        program: Program,
    ) -> Result<(), anyhow::Error> {
        let span = span!(Level::TRACE, "EVAL", task_id, ?program);
        let span_id = span.id();

        if !self.stack.is_empty() {
            // We need to set up a trampoline to return back into `bf_eval`
            self.top_mut().bf_trampoline_arg = None;
            self.top_mut().bf_trampoline = Some(BF_SERVER_EVAL_TRAMPOLINE_RESUME);
        }

        let a = Activation::for_eval(task_id, permissions, player, program, span_id.clone())?;

        self.stack.push(a);

        tracing_enter_span(&span_id, &None);

        Ok(())
    }

    /// Prepare a new stack & call hierarchy for invocation of a forked task.
    /// Called (ultimately) from the scheduler as the result of a fork() call.
    /// We get an activation record which is a copy of where it was borked from, and a new Program
    /// which is the new task's code, derived from a fork vector in the original task.
    pub(crate) async fn exec_fork_vector(
        &mut self,
        fork_request: ForkRequest,
        task_id: usize,
    ) -> Result<(), anyhow::Error> {
        let span = span!(Level::TRACE, "FORK", task_id);
        let span_id = span.id();

        // Set the activation up with the new task ID, and the new code.
        let mut a = fork_request.activation;
        a.span_id = span_id.clone();
        a.task_id = task_id;
        a.program.main_vector = a.program.fork_vectors[fork_request.fork_vector_offset.0].clone();
        a.pc = 0;
        if let Some(task_id_name) = fork_request.task_id {
            a.set_var_offset(task_id_name, v_int(task_id as i64))
                .unwrap();
        }

        // TODO how to set the task_id in the parent activation, as we no longer have a reference
        // to it?
        self.stack = vec![a];

        tracing_enter_span(&span_id, &None);
        Ok(())
    }

    /// Call into a builtin function.
    pub(crate) async fn call_builtin_function<'a>(
        &mut self,
        bf_func_num: usize,
        args: &[Var],
        exec_args: &mut VmExecParams<'a>,
    ) -> Result<ExecutionResult, anyhow::Error> {
        if bf_func_num >= self.builtins.len() {
            return self.raise_error(E_VARNF);
        }
        let bf = self.builtins[bf_func_num].clone();

        debug!(
            "Calling builtin: {}({}) caller_perms: {}",
            BUILTIN_DESCRIPTORS[bf_func_num].name,
            args_literal(args),
            self.top().permissions
        );
        let span = span!(
            Level::TRACE,
            "BF",
            bf_name = BUILTIN_DESCRIPTORS[bf_func_num].name,
            bf_func_num,
            ?args
        );
        span.follows_from(self.top().span_id.clone());

        let _guard = span.enter();

        let args = args.to_vec();

        // Push an activation frame for the builtin function.
        let flags = self.top().verb_info.verbdef().flags();
        self.stack.push(Activation::for_bf_call(
            self.top().task_id,
            bf_func_num,
            BUILTIN_DESCRIPTORS[bf_func_num].name.as_str(),
            args.clone(),
            // We copy the flags from the calling verb, that will determine error handling 'd'
            // behaviour below.
            flags,
            self.top().player,
            span.id(),
        ));
        let mut bf_args = BfCallState {
            vm: self,
            name: BUILTIN_DESCRIPTORS[bf_func_num].name.clone(),
            world_state: exec_args.world_state,
            session: exec_args.session.clone(),
            args,
            scheduler_sender: exec_args.scheduler_sender.clone(),
            ticks_left: exec_args.ticks_left,
            time_left: exec_args.time_left,
        };

        let call_results = match bf.call(&mut bf_args).await {
            Ok(BfRet::Ret(result)) => self.unwind_stack(FinallyReason::Return(result.clone())),
            Ok(BfRet::Error(e)) => self.push_bf_error(e),
            Ok(BfRet::VmInstr(vmi)) => Ok(vmi),
            Err(e) => match e.downcast_ref::<WorldStateError>() {
                Some(e) => {
                    let err_code = e.to_error_code()?;
                    self.push_bf_error(err_code)
                }
                _ => Err(e),
            },
        };

        trace!(?call_results, "Builtin function call complete");
        call_results
    }

    /// We're returning into a builtin function, which is all set up at the top of the stack.
    pub(crate) async fn reenter_builtin_function<'a>(
        &mut self,
        exec_args: VmExecParams<'a>,
    ) -> Result<ExecutionResult, anyhow::Error> {
        trace!(
            bf_index = self.top().bf_index,
            "Reentering builtin function"
        );
        // Functions that did not set a trampoline are assumed to be complete, so we just unwind.
        // Note: If there was an error that required unwinding, we'll have already done that, so
        // we can assume a *value* here not, an error.
        let Some(_) = self.top_mut().bf_trampoline else {
            let return_value = self.top_mut().pop().unwrap();

            return self.unwind_stack(FinallyReason::Return(return_value));
        };

        let bf = self.builtins[self.top().bf_index.unwrap()].clone();
        let verb_name = self.top().verb_name.clone();
        let sessions = exec_args.session.clone();
        let args = self.top().args.clone();
        let mut bf_args = BfCallState {
            vm: self,
            name: verb_name,
            world_state: exec_args.world_state,
            session: sessions,
            args,
            scheduler_sender: exec_args.scheduler_sender.clone(),
            ticks_left: exec_args.ticks_left,
            time_left: exec_args.time_left,
        };

        match bf.call(&mut bf_args).await {
            Ok(BfRet::Ret(result)) => self.unwind_stack(FinallyReason::Return(result.clone())),
            Ok(BfRet::Error(e)) => self.push_bf_error(e),
            Ok(BfRet::VmInstr(vmi)) => Ok(vmi),
            Err(e) => match e.downcast_ref::<WorldStateError>() {
                Some(e) => {
                    let err_code = e.to_error_code()?;
                    self.push_bf_error(err_code)
                }
                _ => Err(e),
            },
        }
    }
}

/// Manually enter a tracing span by its Id.
fn tracing_enter_span(span_id: &Option<span::Id>, follows_span: &Option<span::Id>) {
    if let Some(span_id) = span_id {
        tracing::dispatcher::get_default(|d| {
            if let Some(follows_span) = follows_span {
                d.record_follows_from(span_id, follows_span);
            }
            d.enter(span_id);
        });
    }
}

/// Manually exit a tracing span by its Id.
pub(crate) fn tracing_exit_vm_span(
    span_id: &Option<span::Id>,
    finally_reason: &FinallyReason,
    return_value: &Var,
) {
    if let Some(span_id) = span_id {
        tracing::dispatcher::get_default(|d| {
            // TODO figure out how to get the return value & exit information into the span
            trace!(?finally_reason, ?return_value, "exiting VM span");
            d.exit(span_id);
        });
    }
}