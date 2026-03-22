use anyhow::Result;
use komga_rust::persistence::{
    SqlitePersistenceContext,
    sqlite::{SqliteTempPool, reject_or_quarantine_pool_topology, setup},
};

#[tokio::test]
async fn topology_preserves_bootstrap_visibility() {
    let topology = SqliteTempPool::new("topology-preserves-bootstrap-visibility")
        .await
        .expect("file-backed sqlite pool should be created");
    setup::bootstrap_pool(topology.pool())
        .await
        .expect("bootstrap schema should be applied explicitly");

    let persistence = topology.persistence_context();

    seed_bootstrap_row(&persistence)
        .await
        .expect("bootstrap seed write should succeed");

    let seeded_value = fetch_seeded_value(&persistence)
        .await
        .expect("bootstrap seed read should succeed");

    assert_eq!(seeded_value, 1);

    topology.cleanup().await;
}

#[tokio::test]
async fn pooled_memory_configuration_is_rejected_or_quarantined() {
    let rejection = reject_or_quarantine_pool_topology("sqlite::memory:", 2)
        .expect_err("pooled sqlite::memory: topology must be rejected or quarantined");

    assert!(
        rejection.contains("sqlite::memory:") && rejection.contains("file-backed sqlite topology"),
        "quarantine guard must explicitly reject pooled sqlite::memory: assumptions",
    );
}

async fn seed_bootstrap_row(persistence: &SqlitePersistenceContext) -> Result<()> {
    let mut connection = persistence.pool_connection();
    connection
        .execute("INSERT INTO libraries (id, name) VALUES ('bootstrap-lib', 'Bootstrap Library')")
        .await?;
    Ok(())
}

async fn fetch_seeded_value(persistence: &SqlitePersistenceContext) -> Result<i64> {
    let mut connection = persistence.pool_connection();
    let count = connection
        .fetch_count("SELECT COUNT(*) FROM libraries WHERE id = 'bootstrap-lib'")
        .await?;
    Ok(count)
}
