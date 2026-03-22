use sqlx::{Row, Sqlite, SqlitePool, Transaction};

#[derive(Clone)]
pub struct SqlitePersistenceContext {
    pool: SqlitePool,
}

impl SqlitePersistenceContext {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn pool_connection(&self) -> SqlitePersistenceConnection<'_> {
        SqlitePersistenceConnection::Pool(&self.pool)
    }

    pub async fn begin_unit_of_work(&self) -> Result<SqliteUnitOfWork, sqlx::Error> {
        let transaction = self.pool.begin().await?;
        Ok(SqliteUnitOfWork {
            transaction: Some(transaction),
        })
    }
}

pub struct SqliteUnitOfWork {
    transaction: Option<Transaction<'static, Sqlite>>,
}

impl SqliteUnitOfWork {
    pub fn connection(&mut self) -> SqlitePersistenceConnection<'_> {
        let tx = self
            .transaction
            .as_mut()
            .expect("unit-of-work connection requested after completion");
        SqlitePersistenceConnection::Transaction(tx)
    }

    pub async fn commit(mut self) -> Result<(), sqlx::Error> {
        if let Some(tx) = self.transaction.take() {
            tx.commit().await?;
        }
        Ok(())
    }

    pub async fn rollback(mut self) -> Result<(), sqlx::Error> {
        if let Some(tx) = self.transaction.take() {
            tx.rollback().await?;
        }
        Ok(())
    }
}

pub enum SqlitePersistenceConnection<'a> {
    Pool(&'a SqlitePool),
    Transaction(&'a mut Transaction<'static, Sqlite>),
}

impl SqlitePersistenceConnection<'_> {
    pub async fn execute(&mut self, statement: &str) -> Result<(), sqlx::Error> {
        match self {
            SqlitePersistenceConnection::Pool(pool) => {
                sqlx::query(statement).execute(*pool).await?;
            }
            SqlitePersistenceConnection::Transaction(transaction) => {
                sqlx::query(statement)
                    .execute(transaction.as_mut())
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn fetch_count(&mut self, statement: &str) -> Result<i64, sqlx::Error> {
        let row = match self {
            SqlitePersistenceConnection::Pool(pool) => sqlx::query(statement).fetch_one(*pool).await?,
            SqlitePersistenceConnection::Transaction(transaction) => {
                sqlx::query(statement)
                    .fetch_one(transaction.as_mut())
                    .await?
            }
        };
        Ok(row.get::<i64, _>(0))
    }
}
