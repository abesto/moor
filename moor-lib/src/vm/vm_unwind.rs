use tracing::trace;

use crate::compiler::labels::{Label, Offset};
use crate::values::error::{Error, ErrorPack};
use crate::values::var::VAR_NONE;
use crate::values::var::{v_err, v_int, v_list, v_objid, v_str, Var};
use crate::values::variant::Variant;
use crate::vm::activation::{Activation, HandlerType};
use crate::vm::vm::{ExecutionResult, VM};

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum FinallyReason {
    Fallthrough,
    Raise {
        code: Error,
        msg: String,
        value: Var,
        stack: Vec<Var>,
    },
    Uncaught {
        code: Error,
        msg: String,
        value: Var,
        stack: Vec<Var>,
        backtrace: Vec<Var>,
    },
    Return(Var),
    Abort,
    Exit {
        stack: Offset,
        label: Label,
    },
}
const FINALLY_REASON_RAISE: usize = 0x00;
const FINALLY_REASON_UNCAUGHT: usize = 0x01;
const FINALLY_REASON_RETURN: usize = 0x02;
const FINALLY_REASON_ABORT: usize = 0x03;
const FINALLY_REASON_EXIT: usize = 0x04;
const FINALLY_REASON_FALLTHROUGH: usize = 0x05;

impl FinallyReason {
    pub fn code(&self) -> usize {
        match *self {
            FinallyReason::Fallthrough => FINALLY_REASON_RAISE,
            FinallyReason::Raise { .. } => FINALLY_REASON_RAISE,
            FinallyReason::Uncaught { .. } => FINALLY_REASON_UNCAUGHT,
            FinallyReason::Return(_) => FINALLY_REASON_RETURN,
            FinallyReason::Abort => FINALLY_REASON_ABORT,
            FinallyReason::Exit { .. } => FINALLY_REASON_EXIT,
        }
    }
    pub fn from_code(code: usize) -> FinallyReason {
        match code {
            FINALLY_REASON_RAISE => FinallyReason::Fallthrough,
            FINALLY_REASON_UNCAUGHT => FinallyReason::Fallthrough,
            FINALLY_REASON_RETURN => FinallyReason::Fallthrough,
            FINALLY_REASON_ABORT => FinallyReason::Fallthrough,
            FINALLY_REASON_EXIT => FinallyReason::Fallthrough,
            FINALLY_REASON_FALLTHROUGH => FinallyReason::Fallthrough,
            _ => panic!("Invalid FinallyReason code"),
        }
    }
}

impl VM {
    /// Find the currently active catch handler for a given error code, if any.
    /// Then return the stack offset (from now) of the activation frame containing the handler.
    fn find_handler_active(&self, raise_code: Error) -> Option<usize> {
        // Scan activation frames and their stacks, looking for the first _Catch we can find.
        let mut frame = self.stack.len() - 1;
        loop {
            let activation = &self.stack.get(frame)?;
            for handler in &activation.handler_stack {
                match handler.handler_type {
                    HandlerType::Catch(cnt) => {
                        // Found one, now scan forwards from 'cnt' backwards in the valstack looking for either the first
                        // non-list value, or a list containing the error code.
                        // TODO check for 'cnt' being too large. not sure how to handle, tho
                        // TODO this actually i think is wrong, it needs to pull two values off the stack
                        let i = handler.valstack_pos;
                        for j in (i - cnt)..i {
                            if let Variant::List(codes) = &activation.valstack[j].variant() {
                                if codes.contains(&v_err(raise_code)) {
                                    return Some(frame);
                                }
                            } else {
                                return Some(frame);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if frame == 0 {
                break;
            }
            frame -= 1;
        }
        None
    }

    /// Compose a list of the current stack frames, starting from `start_frame_num` and working
    /// upwards.
    fn make_stack_list(&self, frames: &[Activation], start_frame_num: usize) -> Vec<Var> {
        // TODO LambdaMOO had logic in here about 'root_vector' and 'line_numbers_too' that I haven't included yet.

        let mut stack_list = vec![];
        for (i, a) in frames.iter().rev().enumerate() {
            if i < start_frame_num {
                continue;
            }
            // Produce traceback line for each activation frame and append to stack_list
            // Should include line numbers (if possible), the name of the currently running verb,
            // its definer, its location, and the current player, and 'this'.
            let traceback_entry = vec![
                v_objid(a.this),
                v_str(a.verb_name()),
                v_objid(a.verb_definer()),
                v_objid(a.verb_owner()),
                v_objid(a.player),
                // TODO: find_line_number and add here.
            ];

            stack_list.push(v_list(traceback_entry));
        }
        stack_list
    }

    /// Compose a backtrace list of strings for an error, starting from the current stack frame.
    fn error_backtrace_list(&self, raise_msg: &str) -> Vec<Var> {
        // Walk live activation frames and produce a written representation of a traceback for each
        // frame.
        let mut backtrace_list = vec![];
        for (i, a) in self.stack.iter().rev().enumerate() {
            let mut pieces = vec![];
            if i != 0 {
                pieces.push("... called from ".to_string());
            }
            pieces.push(format!("{}:{}", a.verb_definer(), a.verb_name()));
            if a.verb_definer() != a.this {
                pieces.push(format!(" (this == {})", a.this.0));
            }
            // TODO line number
            if i == 0 {
                pieces.push(format!(": {}", raise_msg));
            }
            // TODO builtin-function name if a builtin

            let piece = pieces.join("");
            backtrace_list.push(v_str(&piece))
        }
        backtrace_list.push(v_str("(End of traceback)"));
        backtrace_list
    }

    /// Raise an error.
    /// Finds the catch handler for the given error if there is one, and unwinds the stack to it.
    /// If there is no handler, creates an 'Uncaught' reason with backtrace, and unwinds with that.
    fn raise_error_pack(&mut self, p: ErrorPack) -> Result<ExecutionResult, anyhow::Error> {
        // Look for first active catch handler's activation frame and its (reverse) offset in the activation stack.
        let handler_activ = self.find_handler_active(p.code);

        let why = if let Some(handler_active_num) = handler_activ {
            FinallyReason::Raise {
                code: p.code,
                msg: p.msg,
                value: p.value,
                stack: self.make_stack_list(&self.stack, handler_active_num),
            }
        } else {
            FinallyReason::Uncaught {
                code: p.code,
                msg: p.msg.clone(),
                value: p.value,
                stack: self.make_stack_list(&self.stack, 0),
                backtrace: self.error_backtrace_list(p.msg.as_str()),
            }
        };

        self.unwind_stack(why)
    }

    /// Push an error to the stack and raise it.
    pub(crate) fn push_error(&mut self, code: Error) -> Result<ExecutionResult, anyhow::Error> {
        trace!("push_error: {:?}", code);
        self.push(&v_err(code));
        self.raise_error_pack(code.make_error_pack(None))
    }

    /// Push an error to the stack with a description and raise it.
    pub(crate) fn push_error_msg(
        &mut self,
        code: Error,
        msg: String,
    ) -> Result<ExecutionResult, anyhow::Error> {
        trace!("push_error_msg: {:?} {:?}", code, msg);
        self.push(&v_err(code));
        self.raise_error_pack(code.make_error_pack(Some(msg)))
    }

    /// Raise an error (without pushing its value to stack)
    pub(crate) fn raise_error(&mut self, code: Error) -> Result<ExecutionResult, anyhow::Error> {
        trace!("raise_error: {:?}", code);
        self.raise_error_pack(code.make_error_pack(None))
    }

    /// Unwind the stack with the given reason and return an execution result back to the VM loop
    /// which makes its way back up to the scheduler.
    /// Contains all the logic for handling the various reasons for exiting a verb execution:
    ///     * Error raises of various kinds
    ///     * Return values
    pub(crate) fn unwind_stack(
        &mut self,
        why: FinallyReason,
    ) -> Result<ExecutionResult, anyhow::Error> {
        trace!("unwind_stack: {:?}", why);
        // Walk activation stack from bottom to top, tossing frames as we go.
        while let Some(a) = self.stack.last_mut() {
            while a.valstack.pop().is_some() {
                // Check the handler stack to see if we've hit a finally or catch handler that
                // was registered for this position in the value stack.
                let Some(handler) = a.pop_applicable_handler() else {
                    continue
                };

                match handler.handler_type {
                    HandlerType::Finally(label) => {
                        let why_num = why.code();
                        if why_num == FinallyReason::Abort.code() {
                            continue;
                        }
                        // Jump to the label pointed to by the finally label and then continue on
                        // executing.
                        a.jump(label);
                        a.push(v_int(why_num as i64));
                        return Ok(ExecutionResult::More);
                    }
                    HandlerType::Catch(_) => {
                        let FinallyReason::Raise { code, value, .. } = &why else {
                            continue
                        };

                        let mut found = false;

                        let Some(handler) = a.pop_applicable_handler() else {
                            continue;
                        };
                        let HandlerType::CatchLabel(pushed_label) = handler.handler_type else {
                            panic!("Expected CatchLabel");
                        };

                        // The value at the top of the stack could be the error codes list.
                        let v = a.pop().expect("Stack underflow");
                        if let Variant::List(error_codes) = v.variant() {
                            if error_codes.contains(&v_err(*code)) {
                                trace!("Matched handler for {:?}", code);
                                a.jump(pushed_label);
                                found = true;
                            }
                        } else {
                            trace!("No match, but jump: {:?}", v);
                            a.jump(pushed_label);
                            found = true;
                        }

                        if found {
                            a.push(value.clone());
                            return Ok(ExecutionResult::More);
                        }
                    }
                    HandlerType::CatchLabel(_) => {
                        panic!("TODO: CatchLabel where we didn't expect it...")
                    }
                }
            }
            if let FinallyReason::Exit { label, .. } = why {
                a.jump(label);
                return Ok(ExecutionResult::More);
            }

            // If we're doing a return, and this is the last activation, we're done and just pass
            // the returned value up out of the interpreter loop.
            // Otherwise pop off this activation, and continue unwinding.
            if let FinallyReason::Return(value) = &why {
                if self.stack.len() == 1 {
                    return Ok(ExecutionResult::Complete(value.clone()));
                }
            }

            if let FinallyReason::Uncaught {
                code: _,
                msg: _,
                value: _,
                stack: _,
                backtrace: _,
            } = &why
            {
                return Ok(ExecutionResult::Exception(why));
            }

            self.stack.pop().expect("Stack underflow");

            if self.stack.is_empty() {
                return Ok(ExecutionResult::Complete(VAR_NONE));
            }
            // TODO builtin function unwinding stuff

            // If it was a return that brought us here, stick it onto the end of the next
            // activation's value stack.
            // (Unless we're the final activation, in which case that should have been handled
            // above)
            if let FinallyReason::Return(value) = why {
                self.push(&value);
                trace!(
                    "Unwinding stack, pushing return value: {} back to verb {}",
                    value,
                    self.top().verb_name()
                );
                return Ok(ExecutionResult::More);
            }
        }

        // We realistically should not get here...
        panic!("Unwound stack to empty, but no exit condition was hit");
    }
}
