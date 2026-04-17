use komga_infrastructure::sqlite::connect_pool;

use super::RuntimeDbPaths;

pub async fn seed_router_read_progress(paths: &RuntimeDbPaths, completed: bool) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(if completed { 10_i64 } else { 1_i64 })
    .bind(completed)
    .execute(&pool)
    .await
    .expect("router contract read-progress row should be inserted");

    pool.close().await;
}

pub async fn seed_router_series_read_progress(
    paths: &RuntimeDbPaths,
    read_count: i64,
    in_progress_count: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(read_count)
    .bind(in_progress_count)
    .execute(&pool)
    .await
    .expect("router contract series read-progress row should be upserted");

    pool.close().await;
}
