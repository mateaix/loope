use loope::{Adapter, LoopOptions, generate_plan, list_adapters};

#[test]
fn default_loop_uses_claude_to_implement_and_codex_to_review() {
    let plan = generate_plan("Add login", LoopOptions::default());

    assert_eq!(plan.requirement, "Add login");
    assert_eq!(plan.steps[0].adapter, Adapter::Claude);
    assert_eq!(plan.steps[0].role.as_str(), "implementer");
    assert_eq!(plan.steps[1].adapter, Adapter::Codex);
    assert_eq!(plan.steps[1].role.as_str(), "reviewer");
    assert!(plan.to_markdown().contains("Claude implements"));
    assert!(plan.to_markdown().contains("Codex reviews"));
}

#[test]
fn design_loop_puts_design_contract_before_implementation() {
    let plan = generate_plan(
        "Build dashboard",
        LoopOptions {
            include_design: true,
            ..LoopOptions::default()
        },
    );

    assert_eq!(plan.steps[0].role.as_str(), "designer");
    assert!(plan.steps[0].expected_artifact.contains("design contract"));
    assert_eq!(plan.steps[1].adapter, Adapter::Claude);
    assert!(plan.to_markdown().contains("Design Contract"));
}

#[test]
fn adapters_include_claude_codex_opencode_and_generic() {
    let adapters = list_adapters();

    assert!(adapters.contains(&Adapter::Claude));
    assert!(adapters.contains(&Adapter::Codex));
    assert!(adapters.contains(&Adapter::OpenCode));
    assert!(adapters.contains(&Adapter::Generic));
}
