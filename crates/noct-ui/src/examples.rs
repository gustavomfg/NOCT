//! EXAMPLE DATA of the first screens. None of it comes from the backend: the shell has no command yet that
//! reads the findings, the model or the vault. The screens show these values and say so. Replace each `EXAMPLE_`
//! constant when its command exists, and remove the prefix only then.

/// How serious a finding is. It is shown as a word as well as a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    /// The word shown next to the finding.
    pub fn label(self) -> &'static str {
        match self {
            Severity::High => "Alta",
            Severity::Medium => "Média",
            Severity::Low => "Baixa",
        }
    }

    /// The classes of the severity label.
    pub fn class(self) -> &'static str {
        match self {
            Severity::High => "sev sev--high",
            Severity::Medium => "sev sev--mid",
            Severity::Low => "sev sev--low",
        }
    }
}

/// A finding of the review mode. EXAMPLE.
pub struct Finding {
    pub severity: Severity,
    pub title: &'static str,
    pub path: &'static str,
    pub body: &'static str,
}

/// EXAMPLE. The model shown in the sidebar.
pub const EXAMPLE_MODEL: &str = "Ollama · local";

/// EXAMPLE. The state of the key vault shown in the sidebar. The backend does not report it yet.
pub const EXAMPLE_VAULT: &str = "Desbloqueado";

/// EXAMPLE. The findings of the review mode.
pub const EXAMPLE_FINDINGS: [Finding; 3] = [
    Finding {
        severity: Severity::High,
        title: "O round trip do keyring falha no Linux",
        path: ".github/workflows/keyring-round-trip.yml",
        body: "O daemon não chega a desbloquear antes do primeiro acesso, e o teste devolve SecureStorageUnavailable.",
    },
    Finding {
        severity: Severity::Medium,
        title: "Listar embeddings reordena o workspace a cada página",
        path: "crates/noct-db",
        body: "Sem estatísticas do SQLite, a travessia de 200 mil linhas levou cerca de 24 s. Com ANALYZE, caiu para menos de 1 s.",
    },
    Finding {
        severity: Severity::Low,
        title: "Workflows herdados ainda usam tags",
        path: ".github/workflows",
        body: "As Actions do Electron seguem fixadas por tag. Fixe cada uma pelo SHA do commit.",
    },
];
