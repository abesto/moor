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

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use moor_db::testing::jepsen::{History, Type, Value};
    use moor_db::tuplebox::tb::{RelationInfo, TupleBox};
    use moor_db::tuplebox::{RelationId, Transaction};
    use moor_values::util::slice_ref::SliceRef;

    fn from_val(value: i64) -> SliceRef {
        SliceRef::from_bytes(&value.to_le_bytes()[..])
    }
    fn to_val(value: SliceRef) -> i64 {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(value.as_slice());
        i64::from_le_bytes(bytes)
    }

    async fn fill_db(
        db: Arc<TupleBox>,
        events: &Vec<History>,
        processes: &mut HashMap<i64, Arc<Transaction>>,
    ) {
        for e in events {
            match e.r#type {
                Type::invoke => {
                    // Start a transaction.
                    let tx = Arc::new(db.clone().start_tx());
                    let existing = processes.insert(e.process, tx.clone());
                    assert!(
                        existing.is_none(),
                        "T{} already exists uncommitted",
                        e.process
                    );
                    // Execute the actions
                    for ev in &e.value {
                        match ev {
                            Value::append(_, register, value) => {
                                // Insert the value into the relation.
                                let relation = RelationId(*register as usize);
                                tx.clone()
                                    .relation(relation)
                                    .await
                                    .insert_tuple(from_val(*value), from_val(*value))
                                    .await
                                    .unwrap();
                            }
                            Value::r(_, register, _) => {
                                let relation = RelationId(*register as usize);

                                // Full-scan.
                                tx.relation(relation)
                                    .await
                                    .predicate_scan(&|_| true)
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
                Type::ok => {
                    let tx = processes.remove(&e.process).unwrap();
                    tx.commit().await.unwrap();
                }
                Type::fail => {
                    let tx = processes.remove(&e.process).unwrap();
                    tx.rollback().await.unwrap();
                }
            }
        }
    }
    pub async fn test_db(dir: PathBuf) -> Arc<TupleBox> {
        // Generate 10 test relations that we'll use for testing.
        let relations = (0..100)
            .map(|i| RelationInfo {
                name: format!("relation_{}", i),
                domain_type_id: 0,
                codomain_type_id: 0,
                secondary_indexed: false,
            })
            .collect::<Vec<_>>();

        TupleBox::new(1 << 24, Some(dir), &relations, 0).await
    }

    // Open a db in a test dir, fill it with some goop, close it, reopen it, and check that the goop is still there.
    #[tokio::test]
    async fn open_reopen() {
        let tmpdir = tempfile::tempdir().unwrap();
        let tmpdir_str = tmpdir.path().to_str().unwrap();
        let tuples = {
            let db = test_db(tmpdir.path().into()).await;
            let lines = include_str!("append-dataset.json")
                .lines()
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>();
            let events = lines
                .iter()
                .map(|l| serde_json::from_str::<History>(l).unwrap());
            let mut processes = HashMap::new();
            fill_db(db.clone(), &events.collect::<Vec<_>>(), &mut processes).await;

            // Go through the relations and scan for what's in there, and remember it.
            let mut expected = HashMap::new();
            for i in 0..100 {
                let relation = RelationId(i);
                let r_tups: Vec<_> = db
                    .with_relation(relation, |r| {
                        let tuples = r.predicate_scan(&|_| true);
                        tuples.iter().map(|t| to_val(t.get().domain())).collect()
                    })
                    .await;
                expected.insert(relation, r_tups);
            }
            db.shutdown().await;
            expected
        };
        // Verify the WAL directory is not empty.
        assert!(std::fs::read_dir(format!("{}/wal", tmpdir_str))
            .unwrap()
            .next()
            .is_some());

        // Now reopen the db and verify that the tuples are still there. We'll do this a few times, to make sure that
        // the recovery is working.
        for _ in 0..5 {
            let db = test_db(tmpdir.path().into()).await;

            // Verify the pages directory is not empty after recovery.
            assert!(std::fs::read_dir(format!("{}/pages", tmpdir_str))
                .unwrap()
                .next()
                .is_some());
            let mut found = HashMap::new();
            // Verify all the tuples in all the relations are there
            for relation in tuples.keys() {
                let expected_tuples = tuples.get(relation).unwrap();
                // Check that the tuples are there, skipping tx layer, just going straight to the relation.
                let tups = db
                    .with_relation(*relation, |r| {
                        let mut tups = vec![];

                        for t in expected_tuples {
                            let t = from_val(*t);
                            let v = r.seek_by_domain(t).unwrap();
                            tups.push(to_val(v.get().domain()));
                        }
                        tups
                    })
                    .await;
                found.insert(*relation, tups);
            }
            assert_eq!(found, tuples);
            db.shutdown().await;
        }
    }
}