use std::collections::HashMap;

use komga_application::operational::HistoryPort;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use crate::database_handle::DatabaseHandle;

#[derive(Clone)]
pub struct HistoryAccess {
    db: DatabaseHandle,
}

impl HistoryAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl HistoryPort for HistoryAccess {
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, String> {
        load_history_page(self.db.read_pool(), page, size, &sorts)
            .await
            .map_err(|e| e.to_string())
    }
}

struct PersistedHistoricalEvent {
    id: String,
    event_type: String,
    book_id: Option<String>,
    series_id: Option<String>,
    timestamp: String,
}

pub(crate) async fn load_history_page(
    pool: &SqlitePool,
    page: u64,
    size: u64,
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    let total_elements = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
        FROM HISTORICAL_EVENT"#,
    )
    .fetch_one(pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let (order_by, sort_payload) = history_sort_details(sorts);
    let mut sql = String::from(
        r#"SELECT ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP
        FROM HISTORICAL_EVENT"#,
    );
    if !order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_by.join(", "));
    }
    sql.push_str(" LIMIT ? OFFSET ?");

    let events = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind((size.min(i64::MAX as u64)) as i64)
        .bind((offset.min(i64::MAX as u64)) as i64)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PersistedHistoricalEvent {
            id: row.get::<String, _>("ID"),
            event_type: row.get::<String, _>("TYPE"),
            book_id: row.get::<Option<String>, _>("BOOK_ID"),
            series_id: row.get::<Option<String>, _>("SERIES_ID"),
            timestamp: row.get::<String, _>("TIMESTAMP"),
        })
        .collect::<Vec<_>>();

    let mut properties_by_id: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();
    if !events.is_empty() {
        let placeholders = std::iter::repeat_n("?", events.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"SELECT ID, "KEY" AS EVENT_KEY, VALUE
            FROM HISTORICAL_EVENT_PROPERTIES
            WHERE ID IN ({placeholders})"#,
        );

        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for event in &events {
            query = query.bind(&event.id);
        }

        let property_rows = query.fetch_all(pool).await?;
        for row in property_rows {
            let event_id = row.get::<String, _>("ID");
            let key = row.get::<String, _>("EVENT_KEY");
            let value = row.get::<String, _>("VALUE");
            properties_by_id
                .entry(event_id)
                .or_default()
                .insert(key, Value::String(value));
        }
    }

    let content = events
        .into_iter()
        .map(|event| {
            let properties = properties_by_id.remove(&event.id).unwrap_or_default();
            json!({
                "id": event.id,
                "type": event.event_type,
                "bookId": event.book_id,
                "seriesId": event.series_id,
                "timestamp": event.timestamp,
                "properties": properties,
            })
        })
        .collect::<Vec<_>>();

    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    Ok(json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": sort_payload.clone(),
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": sort_payload,
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    }))
}

fn history_sort_details(sorts: &[String]) -> (Vec<String>, Value) {
    let order_by = history_order_by(sorts);
    let is_sorted = sorts.is_empty() || !order_by.is_empty();
    let payload = json!({
        "empty": !is_sorted,
        "sorted": is_sorted,
        "unsorted": !is_sorted,
    });

    (order_by, payload)
}

fn history_order_by(sorts: &[String]) -> Vec<String> {
    if sorts.is_empty() {
        return vec!["TIMESTAMP DESC".to_string()];
    }

    sorts
        .iter()
        .filter_map(|sort| history_sort_clause(sort))
        .collect()
}

fn history_sort_clause(sort: &str) -> Option<String> {
    let (property, direction) = match sort.split_once(',') {
        Some((property, direction)) => (property.trim(), direction.trim()),
        None => (sort.trim(), "asc"),
    };

    let field = match property {
        "type" => "TYPE",
        "bookId" => "BOOK_ID",
        "seriesId" => "SERIES_ID",
        "timestamp" => "TIMESTAMP",
        _ => return None,
    };
    let direction = if direction.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };

    Some(format!("{field} {direction}"))
}
