use std::collections::BTreeMap;
use std::collections::HashMap;

use komga_application::operational::{
    HistoryEvent, HistoryPage, HistoryPort, HistorySort, HistorySortDirection, HistorySortProperty,
    HistorySortSelection,
};
use sqlx::{Row, SqlitePool};

use komga_infrastructure_base::DatabaseHandle;

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
        sort: HistorySortSelection,
    ) -> anyhow::Result<HistoryPage> {
        load_history_page(self.db.read_pool(), page, size, sort)
            .await
            .map_err(anyhow::Error::from)
    }
}

struct PersistedHistoricalEvent {
    id: String,
    event_type: String,
    book_id: Option<String>,
    series_id: Option<String>,
    timestamp: String,
}

struct HistorySortPlan {
    order_by: Vec<String>,
    sorted: bool,
}

pub(crate) async fn load_history_page(
    pool: &SqlitePool,
    page: u64,
    size: u64,
    sort: HistorySortSelection,
) -> Result<HistoryPage, sqlx::Error> {
    let total_elements = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
        FROM HISTORICAL_EVENT"#,
    )
    .fetch_one(pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let sort_plan = history_sort_details(sort);
    let mut sql = String::from(
        r#"SELECT ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP
        FROM HISTORICAL_EVENT"#,
    );
    if !sort_plan.order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&sort_plan.order_by.join(", "));
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

    let mut properties_by_id: HashMap<String, BTreeMap<String, String>> = HashMap::new();
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
                .insert(key, value);
        }
    }

    let content = events
        .into_iter()
        .map(|event| {
            let properties = properties_by_id.remove(&event.id).unwrap_or_default();
            HistoryEvent {
                id: event.id,
                event_type: event.event_type,
                book_id: event.book_id,
                series_id: event.series_id,
                timestamp: event.timestamp,
                properties,
            }
        })
        .collect::<Vec<_>>();

    Ok(HistoryPage::new(
        page,
        size,
        total_elements,
        content,
        sort_plan.sorted,
    ))
}

fn history_sort_details(sort: HistorySortSelection) -> HistorySortPlan {
    HistorySortPlan {
        order_by: history_order_by(&sort.sorts),
        sorted: sort.sorted,
    }
}

fn history_order_by(sorts: &[HistorySort]) -> Vec<String> {
    sorts.iter().map(history_sort_clause).collect()
}

fn history_sort_clause(sort: &HistorySort) -> String {
    let field = match sort.property {
        HistorySortProperty::Type => "TYPE",
        HistorySortProperty::BookId => "BOOK_ID",
        HistorySortProperty::SeriesId => "SERIES_ID",
        HistorySortProperty::Timestamp => "TIMESTAMP",
    };
    let direction = match sort.direction {
        HistorySortDirection::Asc => "ASC",
        HistorySortDirection::Desc => "DESC",
    };

    format!("{field} {direction}")
}
