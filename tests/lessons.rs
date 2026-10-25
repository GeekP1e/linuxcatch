use super::*;
use std::collections::HashSet;

#[test]
fn course_has_complete_translations_unique_ids_and_valid_groups() {
    let course = course();
    let mut ids = HashSet::new();
    let groups: HashSet<_> = course
        .groups
        .iter()
        .map(|group| group.id.as_str())
        .collect();
    assert!(course.lessons.len() >= 70);
    for group in &course.groups {
        assert!(!group.title.ru.trim().is_empty());
        assert!(!group.title.en.trim().is_empty());
        assert!(course.lessons.iter().any(|lesson| lesson.group == group.id));
    }
    for lesson in &course.lessons {
        assert!(ids.insert(&lesson.id), "Duplicate lesson: {}", lesson.id);
        assert!(
            groups.contains(lesson.group.as_str()),
            "Unknown group: {}",
            lesson.group
        );
        assert!(matches!(lesson.mode.as_str(), "learn" | "practice"));
        for text in [
            &lesson.title,
            &lesson.explanation,
            &lesson.syntax,
            &lesson.goal,
            &lesson.hint,
            &lesson.success,
        ] {
            assert!(!text.ru.trim().is_empty(), "Missing Russian: {}", lesson.id);
            assert!(!text.en.trim().is_empty(), "Missing English: {}", lesson.id);
            assert!(
                !text
                    .en
                    .chars()
                    .any(|c| ('\u{0400}'..='\u{04ff}').contains(&c)),
                "Untranslated English text: {}",
                lesson.id
            );
        }
        assert!(!lesson.check.is_empty());
        assert!(!lesson.solution.is_empty());
        assert!(
            lesson.start_dir.starts_with("/workspace")
                || lesson.start_dir.starts_with("/home/student")
        );
    }
}

#[test]
fn checks_and_solutions_are_private_and_unknown_lessons_are_rejected() {
    let public = serde_json::to_value(course()).unwrap();
    for lesson in public["lessons"].as_array().unwrap() {
        for key in ["setup", "check", "solution"] {
            assert!(lesson.get(key).is_none());
        }
    }
    assert!(find("../../etc/passwd").is_none());
    for id in ["mkdir", "touch", "mv", "organize"] {
        assert!(find(id).is_some(), "Existing progress ID must remain valid");
    }
}
