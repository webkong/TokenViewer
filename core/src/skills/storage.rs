use rusqlite::params;

use crate::skills::models::{SkillEntry, OrganizedSkill};

pub fn upsert_skills(db: &crate::storage::Database, skills: &[SkillEntry]) -> Result<(), String> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare(
            "INSERT OR REPLACE INTO skills_settings (key, value) VALUES (?1, ?2)",
        )
        .map_err(|e| format!("Failed to prepare upsert: {}", e))?;

    for skill in skills {
        let tags_json = serde_json::to_string(&skill.manifest.tags).unwrap_or_default();
        let agents_json =
            serde_json::to_string(&skill.manifest.compatible_agents).unwrap_or_default();

        stmt.execute(params![
            format!("skill:{}:name", skill.id),
            skill.manifest.name,
        ])
        .map_err(|e| format!("Failed to upsert skill {}: {}", skill.id, e))?;
        let _ = (tags_json, agents_json);
    }
    Ok(())
}

pub fn get_all_skills(db: &crate::storage::Database) -> Result<Vec<OrganizedSkill>, String> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare("SELECT key, value FROM skills_settings WHERE key LIKE 'skill:%' ORDER BY key")
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query skills: {}", e))?;

    let organized = Vec::new();
    for row in rows {
        let _ = row;
    }
    Ok(organized)
}

pub fn set_organized(db: &crate::storage::Database, skill_id: &str) -> Result<(), String> {
    db.conn()
        .execute(
            "INSERT OR REPLACE INTO skills_settings (key, value) VALUES (?1, ?2)",
            params![format!("organized:{}", skill_id), "true"],
        )
        .map_err(|e| format!("Failed to set organized: {}", e))?;
    Ok(())
}

pub fn delete_skill(db: &crate::storage::Database, skill_id: &str) -> Result<(), String> {
    db.conn()
        .execute(
            "DELETE FROM skills_settings WHERE key LIKE ?1",
            params![format!("skill:{}:%", skill_id)],
        )
        .map_err(|e| format!("Failed to delete skill: {}", e))?;
    Ok(())
}
