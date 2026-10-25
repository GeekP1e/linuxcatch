use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Localized {
    pub ru: String,
    pub en: String,
}

impl Localized {
    pub fn new(ru: &str, en: &str) -> Self {
        Self {
            ru: ru.into(),
            en: en.into(),
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Group {
    pub id: String,
    pub title: Localized,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Lesson {
    pub id: String,
    pub group: String,
    pub mode: String,
    pub command: String,
    pub title: Localized,
    pub explanation: Localized,
    pub syntax: Localized,
    pub example: String,
    pub goal: Localized,
    pub hint: Localized,
    pub success: Localized,
    pub start_dir: String,
    #[serde(default)]
    pub lab_model: bool,
    #[serde(default, skip_serializing)]
    pub large_workspace: bool,
    #[serde(skip_serializing)]
    pub setup: String,
    #[serde(skip_serializing)]
    pub check: String,
    #[cfg(test)]
    #[serde(skip_serializing)]
    pub solution: String,
}

#[derive(Deserialize, Serialize)]
pub struct Course {
    pub groups: Vec<Group>,
    pub lessons: Vec<Lesson>,
}

pub fn course() -> &'static Course {
    static COURSE: OnceLock<Course> = OnceLock::new();
    COURSE.get_or_init(|| {
        serde_json::from_str(include_str!("../content/course.json"))
            .expect("Invalid built-in course")
    })
}

pub fn all() -> Vec<Lesson> {
    course().lessons.clone()
}

pub fn find(id: &str) -> Option<Lesson> {
    course()
        .lessons
        .iter()
        .find(|lesson| lesson.id == id)
        .cloned()
}

#[cfg(test)]
#[path = "../../tests/lessons.rs"]
mod tests;
