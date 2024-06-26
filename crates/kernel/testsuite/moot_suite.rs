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

//! Moot is a simple text-based test format for testing the kernel.
//!
//! See example.moot for a full-fledged example

mod common;
use std::{path::Path, sync::Arc};

use common::{create_wiredtiger_db, testsuite_dir};
use moor_db::Database;
use moor_kernel::{
    config::Config,
    tasks::{
        scheduler::{Scheduler, SchedulerError},
        scheduler_test_utils,
        sessions::{NoopClientSession, Session},
    },
};
use moor_moot::{execute_moot_test, MootRunner};
use moor_values::var::{v_none, Objid, Var};

#[cfg(feature = "relbox")]
use common::create_relbox_db;

#[derive(Clone)]
struct SchedulerMootRunner {
    scheduler: Arc<Scheduler>,
    session: Arc<dyn Session>,
}
impl SchedulerMootRunner {
    fn new(scheduler: Arc<Scheduler>, session: Arc<dyn Session>) -> Self {
        Self { scheduler, session }
    }
}
impl MootRunner for SchedulerMootRunner {
    type Value = Var;
    type Error = SchedulerError;

    fn eval<S: Into<String>>(&mut self, player: Objid, command: S) -> Result<Var, SchedulerError> {
        let command = command.into();
        eprintln!("{player} >> ; {command}");
        scheduler_test_utils::call_eval(
            self.scheduler.clone(),
            self.session.clone(),
            player,
            command,
        )
        .inspect(|var| eprintln!("{player} << {var}"))
    }

    fn command<S: AsRef<str>>(&mut self, player: Objid, command: S) -> Result<Var, SchedulerError> {
        eprintln!("{player} >> ; {}", command.as_ref());
        scheduler_test_utils::call_command(
            self.scheduler.clone(),
            self.session.clone(),
            player,
            command.as_ref(),
        )
        .inspect(|var| eprintln!("{player} << {var}"))
    }

    fn none(&self) -> Var {
        v_none()
    }
}

#[cfg(feature = "relbox")]
fn test_relbox(path: &Path) {
    test(create_relbox_db(), path);
}
#[cfg(feature = "relbox")]
test_each_file::test_each_path! { in "./crates/kernel/testsuite/moot" as relbox => test_relbox }

fn test_wiredtiger(path: &Path) {
    test(create_wiredtiger_db(), path);
}
test_each_file::test_each_path! { in "./crates/kernel/testsuite/moot" as wiredtiger => test_wiredtiger }

fn test(db: Arc<dyn Database + Send + Sync>, path: &Path) {
    if path.is_dir() {
        return;
    }
    let scheduler = Arc::new(Scheduler::new(db, Config::default()));
    let loop_scheduler = scheduler.clone();
    let scheduler_loop_jh = std::thread::Builder::new()
        .name("moor-scheduler".to_string())
        .spawn(move || loop_scheduler.run())
        .unwrap();

    execute_moot_test(
        SchedulerMootRunner::new(scheduler.clone(), Arc::new(NoopClientSession::new())),
        path,
    );

    scheduler
        .submit_shutdown(0, Some("Test is done".to_string()))
        .unwrap();
    scheduler_loop_jh.join().unwrap();
}

#[test]
#[ignore = "Useful for debugging; just run a single test"]
fn test_single() {
    // cargo test -p moor-kernel --test moot-suite test_single -- --ignored
    // CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --test moot-suite -- test_single --ignored
    test_wiredtiger(&testsuite_dir().join("moot/single.moot"));
}
