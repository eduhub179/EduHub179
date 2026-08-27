//! Integration tests for `StudentGroupRepositoryPg`.
//!
//! These tests verify the public API of the infrastructure crate
//! with a real PostgreSQL database using `sqlx::test` for automatic
//! transaction management and rollback.
//!
//! Coverage:
//! - Group catalog: `get_by_id`, `get_all`, `save` (create/update/errors).
//! - Membership: `add_member`, `remove_member`, `get_member_ids`, `get_groups_by_student`.
//! - Idempotency of membership operations.
//! - Fail-safe behavior for non-existent groups.
use domain::entities::student_group::StudentGroup;
use domain::entities::user::User;
use domain::errors::DomainError;
use domain::repositories::student_group_repository::StudentGroupRepository;
use domain::repositories::user_repository::UserRepository;
use domain::value_objects::role::UserRole;
use infrastructure::postgres::{StudentGroupRepositoryPg, UserRepositoryPg};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// HELPERS
// ============================================================================

/// Helper: creates a test group with a random ID.
/// Panics if the domain invariants are violated (valid test data should pass).
fn create_test_group(name: &str) -> StudentGroup {
    StudentGroup::try_new(Uuid::new_v4(), name.to_string())
        .expect("Test data should be valid and satisfy domain invariants")
}

/// Helper: creates a test student with a random ID and unique login.
/// Students are required as members of groups (FK to `users`).
fn create_test_student(last_name: &str) -> User {
    User::try_new(
        Uuid::new_v4(),
        format!("student.{}@example.com", Uuid::new_v4()),
        UserRole::Student,
        last_name.to_string(),
        "Test".to_string(),
        None,
        None,
    )
    .expect("Test student data should be valid")
}

// ============================================================================
// TESTS FOR get_by_id
// ============================================================================

/// Test: get_by_id returns StudentGroupNotFound for non-existent ID.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_not_found(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let fake_id = Uuid::new_v4();

    let result = repo.get_by_id(fake_id).await;

    assert!(matches!(result, Err(DomainError::StudentGroupNotFound)));
}

/// Test: get_by_id returns the correct group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_by_id_success(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group = create_test_group("Английский B1");
    repo.save(group.clone()).await.expect("Save should succeed");

    let fetched = repo
        .get_by_id(group.id)
        .await
        .expect("Get by ID should succeed");

    assert_eq!(fetched.id, group.id);
    assert_eq!(fetched.name, "Английский B1");
}

// ============================================================================
// TESTS FOR get_all
// ============================================================================

/// Test: get_all returns empty vec when no groups exist.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_empty(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);

    let result = repo.get_all().await;

    assert_eq!(result.unwrap(), Vec::<StudentGroup>::new());
}

/// Test: get_all returns groups sorted alphabetically by name.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_all_sorted_by_name(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group_fizika = create_test_group("Физика углубленная");
    let group_algebra = create_test_group("Алгебра базовая");
    let group_informatika = create_test_group("Информатика базовая");

    // Insert in random order
    repo.save(group_fizika.clone()).await.unwrap();
    repo.save(group_informatika.clone()).await.unwrap();
    repo.save(group_algebra.clone()).await.unwrap();

    let result = repo.get_all().await.unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].name, "Алгебра базовая");
    assert_eq!(result[1].name, "Информатика базовая");
    assert_eq!(result[2].name, "Физика углубленная");
}

// ============================================================================
// TESTS FOR save (CREATE)
// ============================================================================

/// Test: save creates a new group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_creates_new_group(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group = create_test_group("Химия органическая");

    let result = repo.save(group.clone()).await;

    assert_eq!(result.unwrap(), group);
    let fetched = repo.get_by_id(group.id).await.unwrap();
    assert_eq!(fetched.name, "Химия органическая");
}

/// Test: save allows multiple groups with different names.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_multiple_groups(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group_1 = create_test_group("Математика");
    let group_2 = create_test_group("Литература");
    let group_3 = create_test_group("История");

    assert!(repo.save(group_1).await.is_ok());
    assert!(repo.save(group_2).await.is_ok());
    assert!(repo.save(group_3).await.is_ok());

    let result = repo.get_all().await.unwrap();
    assert_eq!(result.len(), 3);
}

// ============================================================================
// TESTS FOR save (UPDATE / UPSERT)
// ============================================================================

/// Test: save updates an existing group (upsert) when ID matches.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_updates_existing_group(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let original = create_test_group("Старое название");
    repo.save(original.clone()).await.unwrap();

    // Modify the group name but keep the same ID
    let updated = StudentGroup::try_new(original.id, "Новое название".to_string())
        .expect("Valid updated data");
    let result = repo.save(updated.clone()).await;

    assert_eq!(result.unwrap(), updated);
    let fetched = repo.get_by_id(original.id).await.unwrap();
    assert_eq!(fetched.name, "Новое название");
}

// ============================================================================
// TESTS FOR save (ERRORS)
// ============================================================================

/// Test: save raises StudentGroupAlreadyExists when name is duplicate.
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_duplicate_name_raises_error(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group_1 = create_test_group("Геометрия");
    let group_2 = create_test_group("Геометрия"); // Different ID, same name

    repo.save(group_1).await.unwrap();
    let result = repo.save(group_2).await;

    assert!(matches!(
        result,
        Err(DomainError::StudentGroupAlreadyExists)
    ));
}

/// Test: save allows same name if updating the same group (upsert).
#[sqlx::test(migrations = "../../migrations")]
async fn test_save_same_name_same_id_succeeds(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group = create_test_group("Биология");
    repo.save(group.clone()).await.unwrap();

    // Save again with same ID and same name (no-op update)
    let result = repo.save(group.clone()).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().name, "Биология");
}

// ============================================================================
// TESTS FOR add_member
// ============================================================================

/// Test: add_member adds a student to a group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_member_success(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Иванов");
    user_repo.save(student.clone()).await.unwrap();

    // Act
    let result = group_repo.add_member(group.id, student.id).await;

    // Assert
    assert!(result.is_ok());
    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(member_ids, vec![student.id]);
}

/// Test: add_member is idempotent — adding the same student twice is a no-op.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_member_idempotent(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Петров");
    user_repo.save(student.clone()).await.unwrap();

    group_repo.add_member(group.id, student.id).await.unwrap();
    let result = group_repo.add_member(group.id, student.id).await;

    // Second add should not fail and should not duplicate
    assert!(result.is_ok());
    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(member_ids.len(), 1);
}

/// Test: add_member to a non-existent group returns StudentGroupNotFound.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_member_non_existent_group(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let student = create_test_student("Сидоров");
    user_repo.save(student.clone()).await.unwrap();

    let fake_group_id = Uuid::new_v4();
    let result = group_repo.add_member(fake_group_id, student.id).await;

    assert!(matches!(result, Err(DomainError::StudentGroupNotFound)));
}

/// Test: add_member with a non-existent student fails.
///
/// Note: the FK violation on `student_id` is currently mapped to
/// `StudentGroupNotFound` (MVP simplification); the use-case layer is
/// expected to validate student existence before calling this method.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_member_non_existent_student(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let fake_student_id = Uuid::new_v4();
    let result = group_repo.add_member(group.id, fake_student_id).await;

    assert_eq!(result, Err(DomainError::UserNotFound));
}
// ============================================================================
// TESTS FOR add_members
// ============================================================================

/// Test: add_members adds multiple students in a single query.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_members_bulk_success(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool.clone());

    let group = create_test_group("Advanced Mathematics");
    group_repo.save(group.clone()).await.unwrap();

    let student_1 = create_test_student("Ivanov");
    let student_2 = create_test_student("Petrov");
    let student_3 = create_test_student("Sidorov");

    user_repo.save(student_1.clone()).await.unwrap();
    user_repo.save(student_2.clone()).await.unwrap();
    user_repo.save(student_3.clone()).await.unwrap();

    let student_ids = vec![student_1.id, student_2.id, student_3.id];

    // Act
    let result = group_repo.add_members(group.id, &student_ids).await;

    // Assert
    assert!(result.is_ok());
    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(member_ids.len(), 3);
    assert!(member_ids.contains(&student_1.id));
    assert!(member_ids.contains(&student_2.id));
    assert!(member_ids.contains(&student_3.id));
}

/// Test: add_members is idempotent — repeated calls with the same IDs do not duplicate records.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_members_idempotent(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool.clone());

    let group = create_test_group("Physics");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Kuznetsov");
    user_repo.save(student.clone()).await.unwrap();

    // Intentional duplicate in the array to test UNNEST + ON CONFLICT behavior
    let student_ids = vec![student.id, student.id];

    // First call
    group_repo
        .add_members(group.id, &student_ids)
        .await
        .unwrap();
    // Second call (should be a no-op)
    let result = group_repo.add_members(group.id, &student_ids).await;

    assert!(result.is_ok());
    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(member_ids.len(), 1); // Only one unique student should exist
}

/// Test: add_members with an empty array returns Ok and does not hit the database.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_members_empty_array(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());

    let group = create_test_group("Chemistry");
    group_repo.save(group.clone()).await.unwrap();

    let empty_ids: Vec<Uuid> = vec![];
    let result = group_repo.add_members(group.id, &empty_ids).await;

    assert!(result.is_ok());
    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();
    assert!(member_ids.is_empty());
}

/// Test: add_members for a non-existent group returns StudentGroupNotFound.
#[sqlx::test(migrations = "../../migrations")]
async fn test_add_members_non_existent_group(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool.clone());

    let student = create_test_student("Smirnov");
    user_repo.save(student.clone()).await.unwrap();

    let fake_group_id = Uuid::new_v4();
    let student_ids = vec![student.id];

    let result = group_repo.add_members(fake_group_id, &student_ids).await;

    // FK violation (23503) is correctly mapped to StudentGroupNotFound in map_db_error
    assert!(matches!(result, Err(DomainError::StudentGroupNotFound)));
}

/// Test: a student can belong to multiple groups.
#[sqlx::test(migrations = "../../migrations")]
async fn test_student_in_multiple_groups(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group_a = create_test_group("Английский B1");
    let group_b = create_test_group("Информатика базовая");
    group_repo.save(group_a.clone()).await.unwrap();
    group_repo.save(group_b.clone()).await.unwrap();

    let student = create_test_student("Кузнецов");
    user_repo.save(student.clone()).await.unwrap();

    group_repo.add_member(group_a.id, student.id).await.unwrap();
    group_repo.add_member(group_b.id, student.id).await.unwrap();

    let groups = group_repo.get_groups_by_student(student.id).await.unwrap();
    assert_eq!(groups.len(), 2);
}

// ============================================================================
// TESTS FOR remove_member
// ============================================================================

/// Test: remove_member removes a student from a group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_member_success(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Смирнов");
    user_repo.save(student.clone()).await.unwrap();
    group_repo.add_member(group.id, student.id).await.unwrap();

    // Act
    let result = group_repo.remove_member(group.id, student.id).await;

    // Assert
    assert!(result.is_ok());
    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();
    assert!(member_ids.is_empty());
}

/// Test: remove_member is idempotent — removing a non-member is a no-op.
#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_member_non_member_is_noop(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Попов");
    user_repo.save(student.clone()).await.unwrap();

    // Student is NOT a member; removal should still succeed (idempotent).
    let result = group_repo.remove_member(group.id, student.id).await;

    assert!(result.is_ok());
}

/// Test: remove_member from a non-existent group returns StudentGroupNotFound.
///
/// Honors the trait contract: a bare DELETE would not error on a missing group,
/// so the implementation performs an explicit existence check.
#[sqlx::test(migrations = "../../migrations")]
async fn test_remove_member_non_existent_group(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let student = create_test_student("Васильев");
    user_repo.save(student.clone()).await.unwrap();

    let fake_group_id = Uuid::new_v4();
    let result = group_repo.remove_member(fake_group_id, student.id).await;

    assert!(matches!(result, Err(DomainError::StudentGroupNotFound)));
}

// ============================================================================
// TESTS FOR get_member_ids
// ============================================================================

/// Test: get_member_ids returns empty vec for a group with no members.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_member_ids_empty(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let group = create_test_group("Английский B1");
    repo.save(group.clone()).await.unwrap();

    let result = repo.get_member_ids(group.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_member_ids returns empty vec for a non-existent group.
///
/// Consistent with `get_active_students_by_class` in `UserRepositoryPg`:
/// list methods return empty rather than error for a missing parent.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_member_ids_non_existent_group(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);
    let fake_group_id = Uuid::new_v4();

    let result = repo.get_member_ids(fake_group_id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_member_ids returns all members of a group.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_member_ids_returns_all_members(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student_1 = create_test_student("Александров");
    let student_2 = create_test_student("Борисов");
    let student_3 = create_test_student("Григорьев");
    user_repo.save(student_1.clone()).await.unwrap();
    user_repo.save(student_2.clone()).await.unwrap();
    user_repo.save(student_3.clone()).await.unwrap();

    group_repo.add_member(group.id, student_1.id).await.unwrap();
    group_repo.add_member(group.id, student_2.id).await.unwrap();
    group_repo.add_member(group.id, student_3.id).await.unwrap();

    let member_ids = group_repo.get_member_ids(group.id).await.unwrap();

    assert_eq!(member_ids.len(), 3);
    assert!(member_ids.contains(&student_1.id));
    assert!(member_ids.contains(&student_2.id));
    assert!(member_ids.contains(&student_3.id));
}

// ============================================================================
// TESTS FOR get_groups_by_student
// ============================================================================

/// Test: get_groups_by_student returns empty vec when student has no groups.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_groups_by_student_empty(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let student = create_test_student("Одиноков");
    user_repo.save(student.clone()).await.unwrap();

    let result = group_repo.get_groups_by_student(student.id).await.unwrap();

    assert!(result.is_empty());
}

/// Test: get_groups_by_student returns groups sorted by name.
#[sqlx::test(migrations = "../../migrations")]
async fn test_get_groups_by_student_sorted_by_name(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group_fizika = create_test_group("Физика");
    let group_algebra = create_test_group("Алгебра");
    let group_informatika = create_test_group("Информатика");
    group_repo.save(group_fizika.clone()).await.unwrap();
    group_repo.save(group_informatika.clone()).await.unwrap();
    group_repo.save(group_algebra.clone()).await.unwrap();

    let student = create_test_student("Многогруппов");
    user_repo.save(student.clone()).await.unwrap();

    // Insert memberships in random order
    group_repo
        .add_member(group_fizika.id, student.id)
        .await
        .unwrap();
    group_repo
        .add_member(group_informatika.id, student.id)
        .await
        .unwrap();
    group_repo
        .add_member(group_algebra.id, student.id)
        .await
        .unwrap();

    let groups = group_repo.get_groups_by_student(student.id).await.unwrap();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].name, "Алгебра");
    assert_eq!(groups[1].name, "Информатика");
    assert_eq!(groups[2].name, "Физика");
}

// ============================================================================
// COMPLEX SCENARIO TESTS
// ============================================================================

/// Test: full lifecycle — create group, add members, verify, remove member.
#[sqlx::test(migrations = "../../migrations")]
async fn test_full_group_lifecycle(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    // Step 1: Create group
    let group = create_test_group("Спецмат");
    group_repo.save(group.clone()).await.unwrap();

    // Step 2: Create students and add to group
    let student_1 = create_test_student("Первый");
    let student_2 = create_test_student("Второй");
    user_repo.save(student_1.clone()).await.unwrap();
    user_repo.save(student_2.clone()).await.unwrap();

    group_repo.add_member(group.id, student_1.id).await.unwrap();
    group_repo.add_member(group.id, student_2.id).await.unwrap();

    // Step 3: Verify both students are members
    let members = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(members.len(), 2);

    // Step 4: Verify each student sees the group
    let groups_1 = group_repo
        .get_groups_by_student(student_1.id)
        .await
        .unwrap();
    let groups_2 = group_repo
        .get_groups_by_student(student_2.id)
        .await
        .unwrap();
    assert_eq!(groups_1.len(), 1);
    assert_eq!(groups_2.len(), 1);
    assert_eq!(groups_1[0].id, group.id);
    assert_eq!(groups_2[0].id, group.id);

    // Step 5: Remove one student
    group_repo
        .remove_member(group.id, student_1.id)
        .await
        .unwrap();

    // Step 6: Verify removal
    let members_after = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(members_after.len(), 1);
    assert_eq!(members_after[0], student_2.id);

    let groups_1_after = group_repo
        .get_groups_by_student(student_1.id)
        .await
        .unwrap();
    assert!(groups_1_after.is_empty());

    // Step 7: Group is still fetchable
    let fetched = group_repo.get_by_id(group.id).await.unwrap();
    assert_eq!(fetched.name, "Спецмат");
}

/// Test: renaming a group preserves its membership.
#[sqlx::test(migrations = "../../migrations")]
async fn test_rename_group_preserves_membership(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский A2");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Студентов");
    user_repo.save(student.clone()).await.unwrap();
    group_repo.add_member(group.id, student.id).await.unwrap();

    // Rename the group (same ID, new name)
    let renamed =
        StudentGroup::try_new(group.id, "Английский B1".to_string()).expect("Valid new name");
    group_repo.save(renamed.clone()).await.unwrap();

    // Membership must be preserved
    let members = group_repo.get_member_ids(group.id).await.unwrap();
    assert_eq!(members, vec![student.id]);

    let groups = group_repo.get_groups_by_student(student.id).await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "Английский B1");
}

/// Test: multiple groups with Cyrillic names sort correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn test_cyrillic_sorting(pool: PgPool) {
    let repo = StudentGroupRepositoryPg::new(pool);

    let group_a = create_test_group("Английский");
    let group_ya = create_test_group("Японский");
    let group_m = create_test_group("Математика");

    repo.save(group_a.clone()).await.unwrap();
    repo.save(group_ya.clone()).await.unwrap();
    repo.save(group_m.clone()).await.unwrap();

    let result = repo.get_all().await.unwrap();

    assert_eq!(result.len(), 3);
    // PostgreSQL sorts Cyrillic by Unicode code points
    assert_eq!(result[0].name, "Английский");
    assert_eq!(result[1].name, "Математика");
    assert_eq!(result[2].name, "Японский");
}

// ============================================================================
// TESTS FOR has_member
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn test_has_member_non_existent_group(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Попов");
    user_repo.save(student.clone()).await.unwrap();

    let falsh_group = Uuid::new_v4();

    let result = group_repo
        .has_member(falsh_group, student.id)
        .await
        .unwrap();

    assert_eq!(result, false);
}
#[sqlx::test(migrations = "../../migrations")]
async fn test_has_member_no_such_student(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Попов");
    user_repo.save(student.clone()).await.unwrap();

    let result = group_repo.has_member(group.id, student.id).await.unwrap();

    assert_eq!(result, false);
}
#[sqlx::test(migrations = "../../migrations")]
async fn test_has_member_such_student_exists(pool: PgPool) {
    let group_repo = StudentGroupRepositoryPg::new(pool.clone());
    let user_repo = UserRepositoryPg::new(pool);

    let group = create_test_group("Английский B1");
    group_repo.save(group.clone()).await.unwrap();

    let student = create_test_student("Попов");
    user_repo.save(student.clone()).await.unwrap();

    group_repo.add_member(group.id, student.id).await.unwrap();

    let result = group_repo.has_member(group.id, student.id).await.unwrap();

    assert_eq!(result, true);
}
