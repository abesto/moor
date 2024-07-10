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

use crate::model::WorldStateError;
use crate::var::{Error, Var};
use bincode::{Decode, Encode};
use std::fmt::Display;
use std::time::Duration;
use strum::Display;
use thiserror::Error;

/// Possible results returned to waiters on tasks to which they've subscribed.
#[derive(Clone, Debug)]
pub enum TaskResult {
    Success(Var),
    Error(SchedulerError),
}

#[derive(Debug, Clone, Error, Decode, Encode, PartialEq, Display)]
pub enum VerbProgramError {
    NoVerbToProgram,
    CompilationError(Vec<String>),
    DatabaseError,
}

/// Reasons a task might be aborted for a 'limit'
#[derive(Clone, Copy, Debug, Eq, PartialEq, Decode, Encode)]
pub enum AbortLimitReason {
    /// This task hit its allotted tick limit.
    Ticks(usize),
    /// This task hit its allotted time limit.
    Time(Duration),
}

#[derive(Debug, Error, Clone, Decode, Encode, PartialEq)]
pub enum SchedulerError {
    #[error("Scheduler not responding")]
    SchedulerNotResponding,
    #[error("Task not found: {0:?}")]
    TaskNotFound(TaskId),
    #[error("Input request not found: {0:?}")]
    // Using u128 here because Uuid is not bincode-able, but this is just a v4 uuid.
    InputRequestNotFound(u128),
    #[error("Could not start task (internal error)")]
    CouldNotStartTask,
    #[error("Compilation error")]
    CompilationError(#[source] CompileError),
    #[error("Could not start command")]
    CommandExecutionError(#[source] CommandError),
    #[error("Task aborted due to limit: {0:?}")]
    TaskAbortedLimit(AbortLimitReason),
    #[error("Task aborted due to error.")]
    TaskAbortedError,
    #[error("Task aborted due to exception")]
    TaskAbortedException(#[source] UncaughtException),
    #[error("Task aborted due to cancellation.")]
    TaskAbortedCancelled,
    #[error("Unable to program verb {0}")]
    VerbProgramFailed(VerbProgramError),
}

pub type TaskId = usize;

#[derive(Debug, Error, Clone, Decode, Encode, PartialEq)]
pub enum CompileError {
    #[error("Failure to parse string: {0}")]
    StringLexError(String),
    #[error("Failure to parse program: {0}")]
    ParseError(String),
    #[error("Unknown built-in function: {0}")]
    UnknownBuiltinFunction(String),
    #[error("Could not find loop with id: {0}")]
    UnknownLoopLabel(String),
}

#[derive(Clone, Eq, PartialEq, Debug, Decode, Encode)]
pub struct UncaughtException {
    pub code: Error,
    pub msg: String,
    pub value: Var,
    pub stack: Vec<Var>,
    pub backtrace: Vec<Var>,
}

impl Display for UncaughtException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Uncaught exception: {} ({})", self.msg, self.code)
    }
}

impl std::error::Error for UncaughtException {}

/// Errors related to command matching.
#[derive(Debug, Error, Clone, Decode, Encode, Eq, PartialEq)]
pub enum CommandError {
    #[error("Could not parse command")]
    CouldNotParseCommand,
    #[error("Could not find object match for command")]
    NoObjectMatch,
    #[error("Could not find verb match for command")]
    NoCommandMatch,
    #[error("Could not start transaction due to database error")]
    DatabaseError(#[source] WorldStateError),
    #[error("Permission denied")]
    PermissionDenied,
}
