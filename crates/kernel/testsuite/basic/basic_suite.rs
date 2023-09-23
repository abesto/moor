use moor_kernel::compiler::codegen::compile;
use moor_kernel::db::inmemtransient::InMemTransientDatabase;
use moor_kernel::tasks::sessions::NoopClientSession;
use moor_kernel::tasks::vm_test_utils::call_verb;
use moor_kernel::textdump::load_db::textdump_load;
use moor_kernel::vm::opcode::Program;
use moor_values::model::r#match::VerbArgsSpec;
use moor_values::model::verbs::{BinaryType, VerbFlag};
use moor_values::model::world_state::{WorldState, WorldStateSource};
use moor_values::var::objid::Objid;
use moor_values::var::Var;
use moor_values::{AsByteBuffer, SYSTEM_OBJECT};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn testsuite_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir).join("testsuite")
}

// Create a minimal Db to support the test harness.
async fn load_db(db: &mut InMemTransientDatabase) {
    let mut tx = db.tx().unwrap();
    textdump_load(
        &mut tx,
        testsuite_dir().join("Minimal.db").to_str().unwrap(),
    )
    .await
    .expect("Could not load textdump");
    tx.commit().await.unwrap();
}

async fn compile_verbs(db: &mut InMemTransientDatabase, verbs: &[(&str, &Program)]) {
    let mut tx = db.tx().unwrap();
    for (verb_name, program) in verbs {
        let binary = program.make_copy_as_vec();
        tx.add_verb(
            Objid(3),
            SYSTEM_OBJECT,
            vec![(*verb_name).to_string()],
            Objid(3),
            VerbFlag::rx(),
            VerbArgsSpec::this_none_this(),
            binary,
            BinaryType::LambdaMoo18X,
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

async fn eval(db: &mut InMemTransientDatabase, expression: &str) -> Result<Var, anyhow::Error> {
    let binary = compile(format!("return {expression};").as_str()).unwrap();
    compile_verbs(db, &[("test", &binary)]).await;
    let mut state = db.new_world_state().await?;
    let result = call_verb(
        state.as_mut(),
        Arc::new(NoopClientSession::new()),
        "test",
        vec![],
    )
    .await;
    state.commit().await?;
    Ok(result)
}

async fn run_basic_test(test_dir: &str) {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let basic_arith_dir = Path::new(manifest_dir)
        .join("testsuite")
        .join("basic")
        .join(test_dir);

    let test_in = basic_arith_dir.join("test.in");
    let test_out = basic_arith_dir.join("test.out");

    // Read the lines from both files, the first is an input expression, the second the
    // expected output. Both as MOO expressions. # of lines must be identical in each.
    let input = std::fs::read_to_string(test_in).unwrap();
    let in_lines = input.lines();
    let output = std::fs::read_to_string(test_out).unwrap();
    let out_lines = output.lines();
    assert_eq!(in_lines.clone().count(), out_lines.clone().count());

    // Zip
    let zipped = in_lines.zip(out_lines);

    // Frustratingly the tests are not independent, so we need to run them in a single database.
    let mut db = InMemTransientDatabase::new();
    load_db(&mut db).await;
    for (line_num, (input, expected_output)) in zipped.enumerate() {
        let evaluated = eval(&mut db, input).await.unwrap();
        let output = eval(&mut db, expected_output).await.unwrap();
        assert_eq!(evaluated, output, "{test_dir}: line {line_num}: {input}")
    }
}

fn main() {}
#[tokio::test]
async fn basic_arithmetic() {
    run_basic_test("arithmetic").await;
}

#[tokio::test]
async fn basic_value() {
    run_basic_test("value").await;
}

#[tokio::test]
async fn basic_string() {
    run_basic_test("string").await;
}

#[tokio::test]
async fn basic_list() {
    run_basic_test("list").await;
}

#[tokio::test]
async fn basic_property() {
    run_basic_test("property").await;
}

#[tokio::test]
async fn basic_object() {
    run_basic_test("object").await;
}