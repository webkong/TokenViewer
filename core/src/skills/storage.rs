use rusqlite::params;

use crate::skills::models::SkillEntry;

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
        .map_err(|e| format!("Failed to upsert skill name {}: {}", skill.id, e))?;

        stmt.execute(params![
            format!("skill:{}:tags", skill.id),
            tags_json,
        ])
        .map_err(|e| format!("Failed to upsert skill tags {}: {}", skill.id, e))?;

        stmt.execute(params![
            format!("skill:{}:agents", skill.id),
            agents_json,
        ])
        .map_err(|e| format!("Failed to upsert skill agents {}: {}", skill.id, e))?;

        stmt.execute(params![
            format!("skill:{}:description", skill.id),
            skill.manifest.description,
        ])
        .map_err(|e| format!("Failed to upsert skill description {}: {}", skill.id, e))?;

        stmt.execute(params![
            format!("skill:{}:version", skill.id),
            skill.manifest.version,
        ])
        .map_err(|e| format!("Failed to upsert skill version {}: {}", skill.id, e))?;
    }
    Ok(())
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
