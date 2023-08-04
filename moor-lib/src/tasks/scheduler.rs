use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use dashmap::DashMap;
use fast_counter::ConcurrentCounter;
use thiserror::Error;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, instrument, span, trace, warn, Level};

use moor_value::var::objid::Objid;
use moor_value::var::Var;

use crate::compiler::codegen::compile;
use crate::db::match_env::DBMatchEnvironment;
use crate::db::matching::MatchEnvironmentParseMatcher;
use crate::model::world_state::WorldStateSource;
use crate::tasks::command_parse::{parse_command, ParsedCommand};
use crate::tasks::task::{Task, TaskControlMsg, TaskControlResponse};
use crate::tasks::{Sessions, TaskId};
use crate::vm::VM;

struct TaskControl {
    task_id: TaskId,
    control_sender: UnboundedSender<TaskControlMsg>,
    response_receiver: UnboundedReceiver<TaskControlResponse>,
}

pub struct Scheduler {
    running: AtomicBool,
    state_source: Arc<RwLock<dyn WorldStateSource + Send + Sync>>,
    next_task_id: AtomicUsize,
    tasks: DashMap<TaskId, TaskControl>,
    num_started_tasks: ConcurrentCounter,
    num_succeeded_tasks: ConcurrentCounter,
    num_aborted_tasks: ConcurrentCounter,
    num_errored_tasks: ConcurrentCounter,
    num_excepted_tasks: ConcurrentCounter,
}

#[derive(Debug, Eq, PartialEq, Error)]
pub enum SchedulerError {
    #[error("Could not find match for command '{0}': {1:?}")]
    NoCommandMatch(String, ParsedCommand),
}

pub async fn scheduler_loop(scheduler: Arc<RwLock<Scheduler>>) {
    {
        let start_lock = scheduler.write().await;
        start_lock.running.store(true, Ordering::SeqCst);
    }
    loop {
        {
            let mut scheduler = scheduler.write().await;
            if !scheduler.running.load(Ordering::SeqCst) {
                break;
            }
            if let Err(e) = scheduler.do_process().await {
                error!(error = ?e, "Error processing scheduler loop");
            }
        }
        sleep(Duration::from_millis(1)).await;
    }
    info!("Scheduler done.");
}

impl Scheduler {
    pub fn new(state_source: Arc<RwLock<dyn WorldStateSource + Sync + Send>>) -> Self {
        Self {
            running: Default::default(),
            state_source,
            next_task_id: Default::default(),
            tasks: DashMap::new(),
            num_started_tasks: ConcurrentCounter::new(0),
            num_succeeded_tasks: ConcurrentCounter::new(0),
            num_aborted_tasks: ConcurrentCounter::new(0),
            num_errored_tasks: ConcurrentCounter::new(0),
            num_excepted_tasks: ConcurrentCounter::new(0),
        }
    }

    #[instrument(skip(self, sessions))]
    pub async fn submit_command_task(
        &mut self,
        player: Objid,
        command: &str,
        sessions: Arc<RwLock<dyn Sessions>>,
    ) -> Result<TaskId, anyhow::Error> {
        let (vloc, vi, command) = {
            let mut ss = self.state_source.write().await;
            let (mut ws, perms) = ss.new_world_state(player).await?;
            let me = DBMatchEnvironment {
                ws: ws.as_mut(),
                perms: perms.clone(),
            };
            let matcher = MatchEnvironmentParseMatcher { env: me, player };
            let pc = parse_command(command, matcher).await?;
            let loc = ws.location_of(perms.clone(), player).await?;

            match ws.find_command_verb_on(perms.clone(), player, &pc).await? {
                Some(vi) => (player, vi, pc),
                None => match ws.find_command_verb_on(perms.clone(), loc, &pc).await? {
                    Some(vi) => (loc, vi, pc),
                    None => match ws.find_command_verb_on(perms.clone(), pc.dobj, &pc).await? {
                        Some(vi) => (pc.dobj, vi, pc),
                        None => match ws.find_command_verb_on(perms.clone(), pc.iobj, &pc).await? {
                            Some(vi) => (pc.iobj, vi, pc),
                            None => {
                                return Err(anyhow!(SchedulerError::NoCommandMatch(
                                    command.to_string(),
                                    pc
                                )));
                            }
                        },
                    },
                },
            }
        };
        let task_id = self
            .new_task(player, self.state_source.clone(), sessions)
            .await?;

        let Some(task_ref) = self.tasks.get_mut(&task_id) else {
            return Err(anyhow!("Could not find task with id {:?}", task_id));
        };

        trace!(
            "Set up command task {:?} for {:?}, sending StartCommandVerb...",
            task_id,
            command
        );
        // This gets enqueued as the first thing the task sees when it is started.
        task_ref
            .control_sender
            .send(TaskControlMsg::StartCommandVerb {
                player,
                vloc,
                verbinfo: vi,
                command,
            })?;

        Ok(task_id)
    }

    #[instrument(skip(self, sessions))]
    pub async fn submit_verb_task(
        &mut self,
        player: Objid,
        vloc: Objid,
        verb: String,
        args: Vec<Var>,
        sessions: Arc<RwLock<dyn Sessions>>,
    ) -> Result<TaskId, anyhow::Error> {
        let task_id = self
            .new_task(player, self.state_source.clone(), sessions)
            .await?;

        let Some(task_ref) = self.tasks.get_mut(&task_id) else {
            return Err(anyhow!("Could not find task with id {:?}", task_id));
        };

        // This gets enqueued as the first thing the task sees when it is started.
        task_ref.control_sender.send(TaskControlMsg::StartVerb {
            player,
            vloc,
            verb,
            args,
        })?;

        Ok(task_id)
    }

    #[instrument(skip(self, sessions))]
    pub async fn submit_eval_task(
        &mut self,
        player: Objid,
        code: String,
        sessions: Arc<RwLock<dyn Sessions>>,
    ) -> Result<TaskId, anyhow::Error> {
        // Compile the text into a verb.
        let binary = compile(code.as_str())?;

        let task_id = self
            .new_task(player, self.state_source.clone(), sessions)
            .await?;

        let Some(task_ref) = self.tasks.get_mut(&task_id) else {
            return Err(anyhow!("Could not find task with id {:?}", task_id));
        };

        // This gets enqueued as the first thing the task sees when it is started.
        task_ref
            .control_sender
            .send(TaskControlMsg::StartEval { player, binary })?;

        Ok(task_id)
    }

    /// This is expected to be run on a loop, and will process the first task response it sees.
    async fn do_process(&mut self) -> Result<(), anyhow::Error> {
        // Would have preferred a futures::select_all here, but it doesn't seem to be possible to
        // do this without consuming the futures, which we don't want to do.
        let mut to_remove = Vec::new();
        for mut task in self.tasks.iter_mut() {
            match task.response_receiver.try_recv() {
                Ok(msg) => match msg {
                    TaskControlResponse::AbortCancelled => {
                        self.num_aborted_tasks.add(1);

                        warn!(task = task.task_id, "Task cancelled");
                        to_remove.push(task.task_id);
                    }
                    TaskControlResponse::AbortError(e) => {
                        self.num_errored_tasks.add(1);

                        warn!(task = task.task_id, error = ?e, "Task aborted");
                        to_remove.push(task.task_id);
                    }
                    TaskControlResponse::Exception(finally_reason) => {
                        self.num_excepted_tasks.add(1);

                        warn!(task = task.task_id, finally_reason = ?finally_reason, "Task threw exception");
                        to_remove.push(task.task_id);
                    }
                    TaskControlResponse::Success(value) => {
                        self.num_succeeded_tasks.add(1);
                        debug!(task = task.task_id, result = ?value, "Task succeeded");
                        to_remove.push(task.task_id);
                    }
                },
                Err(TryRecvError::Empty) => {}
                Err(e) => {
                    error!(task = task.task_id, error = ?e, "Task sys-errored");
                    to_remove.push(task.task_id);
                    continue;
                }
            }
        }
        for task_id in to_remove {
            self.tasks.remove(&task_id);
        }

        Ok(())
    }

    pub async fn stop(scheduler: Arc<RwLock<Self>>) -> Result<(), anyhow::Error> {
        let scheduler = scheduler.write().await;
        scheduler.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn new_task(
        &mut self,
        player: Objid,
        state_source: Arc<RwLock<dyn WorldStateSource + Send + Sync>>,
        client_connection: Arc<RwLock<dyn Sessions>>,
    ) -> Result<TaskId, anyhow::Error> {
        let (state, perms) = {
            let mut state_source = state_source.write().await;
            state_source.new_world_state(player).await?
        };

        let (control_sender, control_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (response_sender, response_receiver) = tokio::sync::mpsc::unbounded_channel();

        let task_id = self.next_task_id.fetch_add(1, Ordering::SeqCst);

        let task_control = TaskControl {
            task_id,
            control_sender,
            response_receiver,
        };

        self.tasks.insert(task_id, task_control);

        // Spawn the task's thread.
        tokio::spawn(async move {
            span!(
                Level::DEBUG,
                "spawn_task",
                task_id = task_id,
                player = player.to_literal()
            );

            let vm = VM::new();
            let task = Task::new(
                task_id,
                control_receiver,
                response_sender,
                player,
                vm,
                client_connection,
                state,
                perms,
            );

            debug!("Starting up task: {:?}", task_id);
            task.run().await;
            debug!("Completed task: {:?}", task_id);
        });

        self.num_started_tasks.add(1);
        Ok(task_id)
    }

    #[instrument(skip(self))]
    pub async fn abort_task(&mut self, id: TaskId) -> Result<(), anyhow::Error> {
        let task = self
            .tasks
            .get_mut(&id)
            .ok_or(anyhow::anyhow!("Task not found"))?;
        task.control_sender.send(TaskControlMsg::Abort)?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn remove_task(&mut self, id: TaskId) -> Result<(), anyhow::Error> {
        self.tasks
            .remove(&id)
            .ok_or(anyhow::anyhow!("Task not found"))?;
        Ok(())
    }
}
