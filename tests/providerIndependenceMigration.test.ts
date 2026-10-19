import Sqlite from 'better-sqlite3'
import { afterEach, describe, expect, it } from 'vitest'
import { migrateDatabase, migrations } from '../electron/database/migrations'

const databases: Sqlite.Database[] = []
afterEach(() => { for (const database of databases.splice(0)) database.close() })

function previousDatabase() {
  const database = new Sqlite(':memory:')
  databases.push(database)
  database.pragma('foreign_keys = ON')
  migrateDatabase(database, 0, migrations.filter(({ version }) => version <= 31))
  database.exec(`
    INSERT INTO workspaces(path,name,created_at,last_opened_at) VALUES('/project','Project','2026-10-19','2026-10-19');
    INSERT INTO conversations(id,title,workspace,codex_thread_id,created_at,updated_at)
      VALUES('conversation','Conversation','/project','old-thread','2026-10-19','2026-10-19');
    INSERT INTO messages(id,conversation_id,role,content,created_at)
      VALUES('message','conversation','assistant','Preserved response','2026-10-19');
    INSERT INTO provider_configs(id,provider_type,display_name,source,base_url,enabled,requires_authentication,credential_ref,timeout_ms,created_at,updated_at)
      VALUES('provider','openai-compatible','OpenRouter','remote','https://openrouter.ai/api/v1',1,1,'9ba7e635-8746-48bd-a8e9-4609ff1690cb',30000,'2026-10-19','2026-10-19');
    INSERT INTO settings(key,value) VALUES('aiEngine','codex'),('codexPath','/old/cli'),('theme','dark');
    INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at)
      VALUES('execution','/project','conversation','Intent','build','completed','accepted','2026-10-19');
    INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path)
      VALUES('before','execution','/project','before','ready','2026-10-19','/checkpoint/before'),
            ('after','execution','/project','after','ready','2026-10-19','/checkpoint/after');
    INSERT INTO change_sets(id,execution_id,before_checkpoint_id,after_checkpoint_id,status,created_at,updated_at)
      VALUES('set','execution','before','after','accepted','2026-10-19','2026-10-19');
    INSERT INTO changes(id,change_set_id,execution_id,relative_path,operation,origin,status,validation_status,created_at,updated_at)
      VALUES('change','set','execution','file.ts','modify','codex-command','accepted','passed','2026-10-19','2026-10-19'),
            ('file-change','set','execution','other.ts','modify','codex-file-change','pending','unknown','2026-10-19','2026-10-19'),
            ('validation-change','set','execution','test.ts','modify','validation','pending','unknown','2026-10-19','2026-10-19');
    INSERT INTO change_hunks(id,change_id,sequence,base_hash,original_patch,final_patch,status,start_line,end_line)
      VALUES('hunk','change',1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','patch','patch','accepted',1,1);
  `)
  return database
}

describe('provider-independent migration from schema 31', () => {
  it('preserves history, credentials and change decisions while removing CLI state', () => {
    const database = previousDatabase()
    migrateDatabase(database, 31)
    expect(database.pragma('user_version', { simple: true })).toBe(32)
    expect(database.prepare('SELECT * FROM conversations').get()).not.toHaveProperty('codex_thread_id')
    expect(database.prepare('SELECT content FROM messages').get()).toEqual({ content: 'Preserved response' })
    expect(database.prepare('SELECT id,provider_type,credential_ref FROM provider_configs').get()).toEqual({
      id: 'provider', provider_type: 'chat-completions', credential_ref: '9ba7e635-8746-48bd-a8e9-4609ff1690cb',
    })
    expect(database.prepare('SELECT key,value FROM settings').all()).toEqual([{ key: 'theme', value: 'dark' }])
    expect(database.prepare('SELECT origin,status FROM changes ORDER BY id').all()).toEqual([
      { origin: 'agent-command', status: 'accepted' },
      { origin: 'agent-file-change', status: 'pending' },
      { origin: 'validation', status: 'pending' },
    ])
    expect(database.prepare('SELECT change_id,status FROM change_hunks').get()).toEqual({ change_id: 'change', status: 'accepted' })
    expect(database.pragma('foreign_key_check')).toEqual([])
    expect(() => database.exec("UPDATE changes SET origin='codex-command' WHERE id='change'")).toThrow(/CHECK/)
    migrateDatabase(database, 32)
    expect(database.prepare('SELECT COUNT(*) count FROM change_hunks').get()).toEqual({ count: 1 })
  })

  it('rolls back schema and data if a subsequent migration fails', () => {
    const database = previousDatabase()
    expect(() => migrateDatabase(database, 31, [
      migrations.find(({ version }) => version === 32)!,
      { version: 33, up: () => { throw new Error('injected migration failure') } },
    ])).toThrow('injected migration failure')
    expect(database.pragma('user_version', { simple: true })).toBe(31)
    expect(database.prepare('SELECT codex_thread_id FROM conversations').get()).toEqual({ codex_thread_id: 'old-thread' })
    expect(database.prepare('SELECT provider_type,credential_ref FROM provider_configs').get()).toEqual({ provider_type: 'openai-compatible', credential_ref: '9ba7e635-8746-48bd-a8e9-4609ff1690cb' })
    expect(database.prepare("SELECT origin FROM changes WHERE id='change'").get()).toEqual({ origin: 'codex-command' })
    expect(database.prepare("SELECT value FROM settings WHERE key='aiEngine'").get()).toEqual({ value: 'codex' })
    expect(database.pragma('foreign_key_check')).toEqual([])
  })
})
