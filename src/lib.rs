pub mod adapter;
pub mod executor;
pub mod stub;
pub mod subprocess;
pub mod workspace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Adapter {
    Claude,
    Codex,
    OpenCode,
    Generic,
}

impl Adapter {
    pub fn as_str(self) -> &'static str {
        match self {
            Adapter::Claude => "claude",
            Adapter::Codex => "codex",
            Adapter::OpenCode => "opencode",
            Adapter::Generic => "generic",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Adapter::Claude => "Claude",
            Adapter::Codex => "Codex",
            Adapter::OpenCode => "OpenCode",
            Adapter::Generic => "Generic",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Designer,
    Implementer,
    Reviewer,
    Verifier,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Designer => "designer",
            Role::Implementer => "implementer",
            Role::Reviewer => "reviewer",
            Role::Verifier => "verifier",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopOptions {
    pub include_design: bool,
    pub implementer: Adapter,
    pub reviewer: Adapter,
    pub designer: Adapter,
    pub verifier: Adapter,
}

impl Default for LoopOptions {
    fn default() -> Self {
        Self {
            include_design: false,
            implementer: Adapter::Claude,
            reviewer: Adapter::Codex,
            designer: Adapter::Generic,
            verifier: Adapter::Generic,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopStep {
    pub id: usize,
    pub role: Role,
    pub adapter: Adapter,
    pub objective: String,
    pub expected_artifact: String,
    pub gate: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoopPlan {
    pub requirement: String,
    pub steps: Vec<LoopStep>,
}

impl LoopPlan {
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Loope Plan\n\n");
        out.push_str("## Requirement\n\n");
        out.push_str(&self.requirement);
        out.push_str("\n\n");
        out.push_str("## Loop\n\n");

        for step in &self.steps {
            out.push_str(&format!(
                "{}. **{} via {}**: {}\n",
                step.id,
                step.role.as_str(),
                step.adapter.display_name(),
                step.objective
            ));
            out.push_str(&format!(
                "   - Expected artifact: {}\n   - Gate: {}\n",
                step.expected_artifact, step.gate
            ));
        }

        out.push_str("\n## Agent Prompts\n\n");
        for step in &self.steps {
            out.push_str(&format!(
                "### Step {} - {} / {}\n\n{}\n\n",
                step.id,
                step.role.as_str(),
                step.adapter.display_name(),
                prompt_for_step(step, &self.requirement)
            ));
        }

        out
    }
}

pub fn list_adapters() -> Vec<Adapter> {
    vec![
        Adapter::Claude,
        Adapter::Codex,
        Adapter::OpenCode,
        Adapter::Generic,
    ]
}

pub fn generate_plan(requirement: &str, options: LoopOptions) -> LoopPlan {
    let clean_requirement = requirement.trim().to_string();
    let mut steps = Vec::new();

    if options.include_design {
        steps.push(LoopStep {
            id: steps.len() + 1,
            role: Role::Designer,
            adapter: options.designer,
            objective: "Produce a Design Contract before code starts".to_string(),
            expected_artifact: "design contract: user flows, states, components, API/data contracts, acceptance criteria".to_string(),
            gate: "implementation cannot start until the design contract is explicit".to_string(),
        });
    }

    steps.push(LoopStep {
        id: steps.len() + 1,
        role: Role::Implementer,
        adapter: options.implementer,
        objective: format!(
            "{} implements the requested change",
            options.implementer.display_name()
        ),
        expected_artifact: "code patch plus concise implementation notes".to_string(),
        gate: "patch is scoped to the requirement and references the design contract when present"
            .to_string(),
    });

    steps.push(LoopStep {
        id: steps.len() + 1,
        role: Role::Reviewer,
        adapter: options.reviewer,
        objective: format!(
            "{} reviews correctness, regressions, and consistency",
            options.reviewer.display_name()
        ),
        expected_artifact: "review report with blocking findings first".to_string(),
        gate: "review must identify blockers or explicitly state no blocking findings".to_string(),
    });

    steps.push(LoopStep {
        id: steps.len() + 1,
        role: Role::Implementer,
        adapter: options.implementer,
        objective: format!(
            "{} revises based on review findings",
            options.implementer.display_name()
        ),
        expected_artifact: "revision patch and response to each review finding".to_string(),
        gate: "all blocking review findings are addressed or consciously deferred".to_string(),
    });

    let verifier_gate = if options.include_design {
        "tests pass and implementation can verify against the design contract"
    } else {
        "tests pass and the final report lists remaining risks"
    };

    steps.push(LoopStep {
        id: steps.len() + 1,
        role: Role::Verifier,
        adapter: options.verifier,
        objective: "Run verification and produce the final loop report".to_string(),
        expected_artifact: "verification report: commands, outputs, unresolved risks".to_string(),
        gate: verifier_gate.to_string(),
    });

    LoopPlan {
        requirement: clean_requirement,
        steps,
    }
}

pub(crate) fn prompt_for_step(step: &LoopStep, requirement: &str) -> String {
    match step.role {
        Role::Designer => format!(
            "You are the design agent. Create a Design Contract for this requirement:\n\n{}\n\nInclude user flows, UI states, component boundaries, API/data contracts, and acceptance criteria.",
            requirement
        ),
        Role::Implementer => format!(
            "You are the implementation agent using {}. Work only from the requirement and available design/review artifacts.\n\nRequirement:\n{}\n\nReturn a scoped patch and concise notes.",
            step.adapter.display_name(),
            requirement
        ),
        Role::Reviewer => format!(
            "You are the review agent using {}. Review the implementation for bugs, regressions, missing tests, and design consistency.\n\nRequirement:\n{}\n\nPut blocking findings first.",
            step.adapter.display_name(),
            requirement
        ),
        Role::Verifier => format!(
            "You are the verification agent. Run the required checks for this requirement and report exact commands, results, and residual risk:\n\n{}",
            requirement
        ),
    }
}
