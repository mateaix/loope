use std::process::Command;

#[test]
fn cli_plan_prints_default_loop() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let output = Command::new(exe)
        .args(["plan", "Add login"])
        .output()
        .expect("run loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("# Loope Plan"));
    assert!(stdout.contains("Add login"));
    assert!(stdout.contains("Claude implements"));
    assert!(stdout.contains("Codex reviews"));
}

#[test]
fn cli_design_plan_prints_design_contract() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let output = Command::new(exe)
        .args(["plan", "--design", "Add dashboard"])
        .output()
        .expect("run loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("Design Contract"));
    assert!(stdout.contains("verify against the design contract"));
}

#[test]
fn cli_lists_supported_adapters() {
    let exe = env!("CARGO_BIN_EXE_loope");
    let output = Command::new(exe)
        .arg("adapters")
        .output()
        .expect("run loope");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("claude"));
    assert!(stdout.contains("codex"));
    assert!(stdout.contains("opencode"));
    assert!(stdout.contains("generic"));
}
