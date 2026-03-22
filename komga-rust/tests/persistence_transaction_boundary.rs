use komga_rust::persistence::{
    SqlitePersistenceContext,
    sqlite::{SqliteTempPool, setup},
};

#[tokio::test]
async fn transaction_commit_is_owned_by_orchestration_layer() {
    let topology = SqliteTempPool::new("tx-owned-by-orchestration")
        .await
        .expect("sqlite pool should be created");
    setup::bootstrap_pool(topology.pool())
        .await
        .expect("schema bootstrap should succeed");

    let persistence = SqlitePersistenceContext::new(topology.pool().clone());
    let mut setup = persistence.pool_connection();
    setup
        .execute("CREATE TABLE IF NOT EXISTS tx_boundary (value TEXT NOT NULL)")
        .await
        .expect("schema setup should succeed");

    let mut uow = persistence
        .begin_unit_of_work()
        .await
        .expect("begin unit-of-work should succeed");
    {
        let mut tx_connection = uow.connection();
        tx_connection
            .execute("INSERT INTO tx_boundary (value) VALUES ('pending')")
            .await
            .expect("transaction insert should succeed");
    }

    let mut before_commit = persistence.pool_connection();
    let count_before_commit = before_commit
        .fetch_count("SELECT COUNT(*) FROM tx_boundary")
        .await
        .expect("count before commit should succeed");
    assert_eq!(count_before_commit, 0);

    uow.commit()
        .await
        .expect("orchestrator-owned commit should succeed");

    let mut after_commit = persistence.pool_connection();
    let count_after_commit = after_commit
        .fetch_count("SELECT COUNT(*) FROM tx_boundary")
        .await
        .expect("count after commit should succeed");
    assert_eq!(count_after_commit, 1);

    topology.cleanup().await;
}
