//! Integração entre catálogo, grafo e recomendação sobre uma base sintética maior.

use std::collections::HashSet;

use megastore::graph::Graph;
use megastore::recommend::{self, Reason};
use megastore::seed::{self, SeedConfig};
use megastore::store::Purchase;

fn config() -> SeedConfig {
    SeedConfig { products: 2_000, customers: 400, purchases_per_customer: 6, seed: 3 }
}

#[test]
fn recomendacoes_de_clientes_sao_unicas_inéditas_e_ordenadas() {
    let store = seed::generate(&config());
    let graph = Graph::from_store(&store);
    for customer in store.customers() {
        let outcome = recommend::for_customer(&graph, customer.id, 10, 3).unwrap();
        let purchased: HashSet<u32> = store.purchases_of(customer.id).map(|p| p.product_id).collect();
        let ids: Vec<u32> = outcome.items.iter().map(|item| item.product_id).collect();
        let unique: HashSet<u32> = ids.iter().copied().collect();

        assert!(ids.len() <= 10);
        assert_eq!(unique.len(), ids.len(), "recomendação duplicada para o cliente {}", customer.id);
        assert!(unique.is_disjoint(&purchased), "recomendou produto já comprado ao cliente {}", customer.id);
        assert!(ids.iter().all(|id| store.product(*id).is_some()));

        let graph_scores: Vec<f32> = outcome
            .items
            .iter()
            .filter(|item| !matches!(item.reason, Reason::SameCategory { .. }))
            .map(|item| item.score)
            .collect();
        assert!(graph_scores.windows(2).all(|pair| pair[0] >= pair[1]));
    }
}

#[test]
fn recomendacoes_de_produto_nao_incluem_o_proprio_produto() {
    let store = seed::generate(&config());
    let graph = Graph::from_store(&store);
    for product in store.products().iter().take(300) {
        let outcome = recommend::for_product(&graph, product.id, 10, 2).unwrap();
        let ids: HashSet<u32> = outcome.items.iter().map(|item| item.product_id).collect();
        assert_eq!(ids.len(), outcome.items.len());
        assert!(!ids.contains(&product.id));
    }
}

#[test]
fn nova_compra_muda_as_recomendacoes() {
    let mut store = seed::generate(&SeedConfig::default());
    let customer = store.add_customer("Cliente Novo").unwrap();
    let before = recommend::for_customer(&Graph::from_store(&store), customer, 5, 3).unwrap();
    assert!(before.items.is_empty(), "cliente sem histórico não tem de onde partir");

    let product = store.products()[0].id;
    store.add_purchase(Purchase { customer_id: customer, product_id: product, quantity: 1, rating: Some(5) }).unwrap();
    let after = recommend::for_customer(&Graph::from_store(&store), customer, 5, 3).unwrap();
    assert!(!after.items.is_empty());
    assert!(after.items.iter().all(|item| item.product_id != product));
}

#[test]
fn resultado_e_deterministico() {
    let store = seed::generate(&config());
    let (a, b) = (Graph::from_store(&store), Graph::from_store(&store));
    for id in 1..=50 {
        assert_eq!(recommend::for_customer(&a, id, 10, 3).unwrap().items, recommend::for_customer(&b, id, 10, 3).unwrap().items);
    }
}
