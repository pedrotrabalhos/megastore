//! Medição de tempo com volumes diferentes de dados sintéticos.

use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::graph::Graph;
use crate::recommend;
use crate::seed::{self, Rng, SeedConfig};

#[derive(Debug, Clone)]
pub struct BenchRow {
    pub products: usize,
    pub customers: usize,
    pub purchases: usize,
    pub nodes: usize,
    pub edges: usize,
    pub build: Duration,
    /// Tempo médio de uma consulta de produto por id via `HashMap`.
    pub hash_lookup: Duration,
    /// Tempo médio da mesma consulta por busca linear no vetor, para comparação.
    pub linear_lookup: Duration,
    pub recommend_customer: Duration,
    pub recommend_product: Duration,
    pub visited_average: f64,
    pub list_bytes: u128,
    pub matrix_bytes: u128,
}

/// Gera `products` produtos (clientes = produtos ÷ 5, ~8 compras por cliente),
/// monta o grafo e mede `samples` consultas e recomendações aleatórias.
pub fn run(products: usize, samples: usize, seed: u64) -> BenchRow {
    let products = products.max(1);
    let samples = samples.max(1);
    let config = SeedConfig { products, customers: (products / 5).max(10), purchases_per_customer: 8, seed };
    let store = seed::generate(&config);

    let started = Instant::now();
    let graph = Graph::from_store(&store);
    let build = started.elapsed();

    let mut rng = Rng::new(seed ^ 0xBE7C);
    let product_ids: Vec<u32> = (0..samples).map(|_| rng.below(products) as u32 + 1).collect();
    let customer_ids: Vec<u32> = (0..samples).map(|_| rng.below(config.customers) as u32 + 1).collect();

    let started = Instant::now();
    for id in &product_ids {
        black_box(store.product(*id));
    }
    let hash_lookup = started.elapsed() / samples as u32;

    let started = Instant::now();
    for id in &product_ids {
        black_box(store.products().iter().find(|product| product.id == *id));
    }
    let linear_lookup = started.elapsed() / samples as u32;

    let mut visited = 0;
    let started = Instant::now();
    for id in &customer_ids {
        visited += black_box(recommend::for_customer(&graph, *id, 10, 3)).map_or(0, |outcome| outcome.visited);
    }
    let recommend_customer = started.elapsed() / samples as u32;

    let started = Instant::now();
    for id in &product_ids {
        black_box(recommend::for_product(&graph, *id, 10, 2));
    }
    let recommend_product = started.elapsed() / samples as u32;

    let memory = graph.memory_estimate();
    BenchRow {
        products,
        customers: config.customers,
        purchases: store.purchases().len(),
        nodes: graph.node_count(),
        edges: graph.edge_count(),
        build,
        hash_lookup,
        linear_lookup,
        recommend_customer,
        recommend_product,
        visited_average: visited as f64 / samples as f64,
        list_bytes: memory.adjacency_list,
        matrix_bytes: memory.adjacency_matrix,
    }
}
