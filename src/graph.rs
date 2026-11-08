//! Grafo heterogêneo, não-direcionado e ponderado, em lista de adjacência.
//!
//! Vértices são clientes, produtos e categorias. Cada vértice recebe um índice denso
//! (0..n), e um `HashMap<Node, usize>` traduz o identificador do domínio para esse índice.
//! A adjacência é um `Vec<Vec<Edge>>`: memória O(V + E), contra O(V²) de uma matriz,
//! o que importa num grafo esparso com milhões de vértices.

use std::collections::HashMap;
use std::mem::size_of;

use crate::store::Store;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Node {
    Customer(u32),
    Product(u32),
    Category(String),
}

impl Node {
    pub fn is_category(&self) -> bool {
        matches!(self, Node::Category(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub to: usize,
    pub weight: f32,
}

#[derive(Debug, Default)]
pub struct Graph {
    nodes: Vec<Node>,
    adjacency: Vec<Vec<Edge>>,
    index: HashMap<Node, usize>,
    edges: usize,
}

/// Memória estimada (bytes) das duas representações para o grafo atual.
#[derive(Debug, Clone, Copy)]
pub struct MemoryEstimate {
    pub adjacency_list: u128,
    pub adjacency_matrix: u128,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(nodes: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(nodes),
            adjacency: Vec::with_capacity(nodes),
            index: HashMap::with_capacity(nodes),
            edges: 0,
        }
    }

    /// Monta o grafo a partir do catálogo em O(V + E log E).
    ///
    /// Arestas produto–categoria têm peso 1. Compras repetidas do mesmo par
    /// cliente–produto são somadas numa única aresta antes da inserção.
    pub fn from_store(store: &Store) -> Self {
        let mut graph = Graph::with_capacity(store.products().len() + store.customers().len() + 16);
        for product in store.products() {
            let product_node = graph.add_node(Node::Product(product.id));
            let category_node = graph.add_node(Node::Category(product.category.clone()));
            graph.push_edge(product_node, category_node, 1.0);
        }
        for customer in store.customers() {
            graph.add_node(Node::Customer(customer.id));
        }

        let mut weights: HashMap<(u32, u32), f32> = HashMap::new();
        for purchase in store.purchases() {
            *weights.entry((purchase.customer_id, purchase.product_id)).or_default() += purchase.weight();
        }
        // Ordena para que a ordem das adjacências (e, portanto, os resultados) seja determinística.
        let mut weights: Vec<_> = weights.into_iter().collect();
        weights.sort_unstable_by_key(|&(pair, _)| pair);
        for ((customer_id, product_id), weight) in weights {
            let customer = graph.index_of(&Node::Customer(customer_id)).expect("cliente validado pelo Store");
            let product = graph.index_of(&Node::Product(product_id)).expect("produto validado pelo Store");
            graph.push_edge(customer, product, weight);
        }
        graph
    }

    /// Insere o vértice se ainda não existe e devolve seu índice. O(1) médio.
    pub fn add_node(&mut self, node: Node) -> usize {
        if let Some(&index) = self.index.get(&node) {
            return index;
        }
        let index = self.nodes.len();
        self.index.insert(node.clone(), index);
        self.nodes.push(node);
        self.adjacency.push(Vec::new());
        index
    }

    pub fn index_of(&self, node: &Node) -> Option<usize> {
        self.index.get(node).copied()
    }

    pub fn node(&self, index: usize) -> &Node {
        &self.nodes[index]
    }

    pub fn neighbors(&self, index: usize) -> &[Edge] {
        &self.adjacency[index]
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges
    }

    pub fn nodes(&self) -> impl Iterator<Item = (usize, &Node)> {
        self.nodes.iter().enumerate()
    }

    /// Conecta `a` e `b`; se a aresta já existe, soma o peso. O(grau(a) + grau(b)).
    /// Laços (a == b) são ignorados.
    pub fn connect(&mut self, a: usize, b: usize, weight: f32) {
        if a == b {
            return;
        }
        match self.adjacency[a].iter_mut().find(|edge| edge.to == b) {
            Some(edge) => {
                edge.weight += weight;
                let back = self.adjacency[b].iter_mut().find(|edge| edge.to == a).expect("aresta simétrica");
                back.weight += weight;
            }
            None => self.push_edge(a, b, weight),
        }
    }

    /// Insere a aresta nos dois sentidos sem procurar duplicata. O(1) amortizado.
    /// Usado na construção em lote, quando as arestas já chegam agregadas.
    fn push_edge(&mut self, a: usize, b: usize, weight: f32) {
        self.adjacency[a].push(Edge { to: b, weight });
        self.adjacency[b].push(Edge { to: a, weight });
        self.edges += 1;
    }

    /// Remove a aresta entre `a` e `b`. Devolve se ela existia. O(grau(a) + grau(b)).
    pub fn disconnect(&mut self, a: usize, b: usize) -> bool {
        let before = self.adjacency[a].len();
        self.adjacency[a].retain(|edge| edge.to != b);
        if self.adjacency[a].len() == before {
            return false;
        }
        self.adjacency[b].retain(|edge| edge.to != a);
        self.edges -= 1;
        true
    }

    pub fn edge_weight(&self, a: usize, b: usize) -> Option<f32> {
        self.adjacency[a].iter().find(|edge| edge.to == b).map(|edge| edge.weight)
    }

    /// Estimativa grosseira: vértices + índice + 2 entradas por aresta (lista),
    /// contra V² pesos `f32` (matriz).
    pub fn memory_estimate(&self) -> MemoryEstimate {
        let vertices = self.nodes.len() as u128;
        let per_vertex = (size_of::<Node>() * 2 + size_of::<Vec<Edge>>() + size_of::<usize>()) as u128;
        let adjacency_list = vertices * per_vertex + 2 * self.edges as u128 * size_of::<Edge>() as u128;
        let adjacency_matrix = vertices * vertices * size_of::<f32>() as u128;
        MemoryEstimate { adjacency_list, adjacency_matrix }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Purchase;

    #[test]
    fn add_node_e_idempotente() {
        let mut graph = Graph::new();
        let a = graph.add_node(Node::Product(1));
        let b = graph.add_node(Node::Product(1));
        assert_eq!(a, b);
        assert_eq!(graph.node_count(), 1);
        assert_eq!(graph.index_of(&Node::Product(1)), Some(a));
        assert_eq!(graph.index_of(&Node::Customer(1)), None);
    }

    #[test]
    fn connect_e_nao_direcionado_e_acumula_peso() {
        let mut graph = Graph::new();
        let customer = graph.add_node(Node::Customer(1));
        let product = graph.add_node(Node::Product(1));
        graph.connect(customer, product, 1.0);
        graph.connect(product, customer, 2.0);
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.edge_weight(customer, product), Some(3.0));
        assert_eq!(graph.edge_weight(product, customer), Some(3.0));
    }

    #[test]
    fn ignora_laco_e_remove_aresta() {
        let mut graph = Graph::new();
        let a = graph.add_node(Node::Customer(1));
        let b = graph.add_node(Node::Product(1));
        graph.connect(a, a, 1.0);
        assert_eq!(graph.edge_count(), 0);
        graph.connect(a, b, 1.0);
        assert!(graph.disconnect(b, a));
        assert!(!graph.disconnect(a, b));
        assert!(graph.neighbors(a).is_empty() && graph.neighbors(b).is_empty());
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn from_store_agrega_compras_repetidas() {
        let mut store = Store::default();
        store.add_product("Notebook", "Eletrônicos", 1.0).unwrap();
        store.add_product("Fone", "Eletrônicos", 1.0).unwrap();
        store.add_customer("Ana").unwrap();
        for _ in 0..2 {
            store.add_purchase(Purchase { customer_id: 1, product_id: 1, quantity: 1, rating: None }).unwrap();
        }
        let graph = Graph::from_store(&store);
        // 2 produtos + 1 categoria + 1 cliente; 2 arestas produto–categoria + 1 cliente–produto.
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.edge_count(), 3);
        let customer = graph.index_of(&Node::Customer(1)).unwrap();
        let product = graph.index_of(&Node::Product(1)).unwrap();
        assert_eq!(graph.edge_weight(customer, product), Some(2.0));
    }

    #[test]
    fn matriz_cresce_quadraticamente() {
        let mut graph = Graph::new();
        for id in 0..1000 {
            graph.add_node(Node::Product(id));
        }
        let memory = graph.memory_estimate();
        assert_eq!(memory.adjacency_matrix, 1000 * 1000 * 4);
        assert!(memory.adjacency_list < memory.adjacency_matrix / 10);
    }
}
