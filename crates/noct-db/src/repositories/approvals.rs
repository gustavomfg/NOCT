//! Audit trail for explicit execution approvals. Port of `ApprovalRepository.ts`.

use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::Result;
use crate::clock::iso_now;
use crate::text::utf16_prefix;

/// Commands are stored up to this many UTF-16 code units.
const COMMAND_UTF16_UNITS: usize = 4_000;

pub struct Approvals<'a> {
    conn: &'a Connection,
}

impl<'a> Approvals<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn record(
        &self,
        key: &str,
        accepted: bool,
        command: Option<&str>,
        risk: Option<&str>,
    ) -> Result<()> {
        let decision = if accepted { "accepted" } else { "declined" };
        let command = command.map(|text| utf16_prefix(text, COMMAND_UTF16_UNITS));
        self.conn.execute(
            "INSERT INTO approval_audit(id, approval_key, decision, command, risk, created_at) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                key,
                decision,
                command,
                risk,
                iso_now()?
            ],
        )?;
        Ok(())
    }
}
