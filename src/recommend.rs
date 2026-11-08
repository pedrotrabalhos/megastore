//! Recomendação por BFS com profundidade limitada.
//!
//! A partir de um cliente: cliente → produtos que comprou (nível 1) → outros clientes que
//! compraram esses produtos (nível 2) → produtos desses clientes (nível 3).
//! A partir de um produto: produto → compradores (nível 1) → outros produtos deles (nível 2).
//!
//! O score de cada vértice é a probabilidade de um passeio aleatório, que a cada passo segue
//! uma aresta proporcionalmente ao peso, chegar até ele. A BFS processa um nível inteiro antes
//! do próximo, então todas as contribuições de um vértice já foram somadas quando ele sai
//! da fila. No fim, o score é dividido pela raiz do número de compradores do produto, para
//! não recomendar sempre os mais vendidos.

use std::cmp::{Ordering, Reverse};
use std::collections::hash_map::Entry;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use crate::graph::{Graph, Node};

#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// Clientes que compraram `via` (comprado por este cliente) também compraram o produto.
    BoughtWith { via: u32 },
    /// `shared` clientes compraram o produto de origem e este.
    AlsoBought { shared: u32 },
    /// Conexão mais distante que os vizinhos de compra diretos.
    Indirect { distance: u8 },
    /// Preenchimento por categoria quando o grafo de compras não basta.
    SameCategory { category: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub product_id: u32,
    pub score: f32,
    pub reason: Reason,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub items: Vec<Recommendation>,
    /// Vértices alcançados pela BFS (sem contar categorias).
    pub visited: usize,
}

/// Recomenda até `k` produtos para um cliente. `None` se o cliente não existe no grafo.
pub fn for_customer(graph: &Graph, customer_id: u32, k: usize, depth: u8) -> Option<Outcome> {
    let start = graph.index_of(&Node::Customer(customer_id))?;
    // HashSet dos produtos já comprados: nunca recomendar o que o cliente já tem.
    let purchased: HashSet<usize> = graph.neighbors(start).iter().map(|edge| edge.to).collect();
    Some(recommend(graph, start, &purchased, k, depth))
}

/// Recomenda até `k` produtos relacionados a um produto. `None` se o produto não existe.
pub fn for_product(graph: &Graph, product_id: u32, k: usize, depth: u8) -> Option<Outcome> {
    let start = graph.index_of(&Node::Product(product_id))?;
    Some(recommend(graph, start, &HashSet::from([start]), k, depth))
}

#[derive(Debug, Clone, Copy)]
struct Visit {
    depth: u8,
    score: f32,
    /// Número de caminhos mínimos da origem até o vértice.
    paths: u32,
    /// Vértice do nível 1 pelo qual chegou a maior contribuição (explica a recomendação).
    origin: usize,
    best: f32,
}

/// BFS limitada a `max_depth` níveis que ignora vértices de categoria.
/// Tempo O(soma dos graus dos vértices alcançados); cada vértice entra na fila uma única vez.
fn walk(graph: &Graph, start: usize, max_depth: u8) -> HashMap<usize, Visit> {
    let mut visits = HashMap::new();
    visits.insert(start, Visit { depth: 0, score: 1.0, paths: 1, origin: start, best: 0.0 });
    let mut queue = VecDeque::from([start]);

    while let Some(current) = queue.pop_front() {
        let visit = visits[&current];
        if visit.depth >= max_depth {
            continue;
        }
        let edges = graph.neighbors(current).iter().filter(|edge| !graph.node(edge.to).is_category());
        let total: f32 = edges.clone().map(|edge| edge.weight).sum();
        if total <= 0.0 {
            continue;
        }
        for edge in edges {
            let share = visit.score * edge.weight / total;
            let origin = if visit.depth == 0 { edge.to } else { visit.origin };
            match visits.entry(edge.to) {
                Entry::Vacant(slot) => {
                    slot.insert(Visit { depth: visit.depth + 1, score: share, paths: visit.paths, origin, best: share });
                    queue.push_back(edge.to);
                }
                // Já descoberto no próximo nível por outro caminho: acumula. Se foi descoberto
                // num nível anterior (ou é a origem), a aresta volta para trás e é ignorada.
                Entry::Occupied(mut slot) => {
                    let next = slot.get_mut();
                    if next.depth == visit.depth + 1 {
                        next.score += share;
                        next.paths = next.paths.saturating_add(visit.paths);
                        if share > next.best {
                            next.best = share;
                            next.origin = origin;
                        }
                    }
                }
            }
        }
    }
    visits
}

fn recommend(graph: &Graph, start: usize, exclude: &HashSet<usize>, k: usize, depth: u8) -> Outcome {
    let visits = walk(graph, start, depth);
    let from_customer = matches!(graph.node(start), Node::Customer(_));

    let candidates = visits.iter().filter(|(node, _)| !exclude.contains(*node)).filter_map(|(&node, visit)| {
        let product_id = product_id(graph, node)?;
        let popularity = (buyers(graph, node).max(1) as f32).sqrt();
        Some(Ranked { score: visit.score / popularity, product_id, node })
    });
    let ranked = top_k(candidates, k);

    // Cada vértice aparece uma única vez em `visits`; `chosen` garante o mesmo no preenchimento.
    let mut chosen: HashSet<usize> = ranked.iter().map(|r| r.node).collect();
    let mut items: Vec<Recommendation> = ranked
        .iter()
        .map(|r| {
            let visit = &visits[&r.node];
            let reason = match (from_customer, visit.depth) {
                (true, _) => Reason::BoughtWith { via: product_id(graph, visit.origin).unwrap_or_default() },
                (false, 2) => Reason::AlsoBought { shared: visit.paths },
                (false, distance) => Reason::Indirect { distance },
            };
            Recommendation { product_id: r.product_id, score: r.score, reason }
        })
        .collect();

    if items.len() < k {
        let references: Vec<usize> = if from_customer {
            graph.neighbors(start).iter().map(|edge| edge.to).collect()
        } else {
            vec![start]
        };
        fill_by_category(graph, &references, exclude, &mut chosen, k - items.len(), &mut items);
    }
    Outcome { items, visited: visits.len() }
}

/// Completa a lista com os produtos mais comprados das categorias de referência
/// (percorrendo produto → categoria → produtos), resolvendo o "cold start" de produtos sem compras.
fn fill_by_category(
    graph: &Graph,
    references: &[usize],
    exclude: &HashSet<usize>,
    chosen: &mut HashSet<usize>,
    missing: usize,
    items: &mut Vec<Recommendation>,
) {
    let categories: HashSet<usize> = references
        .iter()
        .flat_map(|&product| graph.neighbors(product))
        .filter(|edge| graph.node(edge.to).is_category())
        .map(|edge| edge.to)
        .collect();
    let candidates = categories
        .iter()
        .flat_map(|&category| graph.neighbors(category))
        .map(|edge| edge.to)
        .filter(|node| !exclude.contains(node) && !chosen.contains(node))
        .filter_map(|node| Some(Ranked { score: buyers(graph, node) as f32, product_id: product_id(graph, node)?, node }));

    for ranked in top_k(candidates, missing) {
        chosen.insert(ranked.node);
        let category = category_of(graph, ranked.node).unwrap_or_default().to_string();
        items.push(Recommendation { product_id: ranked.product_id, score: 0.0, reason: Reason::SameCategory { category } });
    }
}

fn product_id(graph: &Graph, node: usize) -> Option<u32> {
    match graph.node(node) {
        Node::Product(id) => Some(*id),
        _ => None,
    }
}

fn buyers(graph: &Graph, product: usize) -> usize {
    graph.neighbors(product).iter().filter(|edge| matches!(graph.node(edge.to), Node::Customer(_))).count()
}

fn category_of(graph: &Graph, product: usize) -> Option<&str> {
    graph.neighbors(product).iter().find_map(|edge| match graph.node(edge.to) {
        Node::Category(name) => Some(name.as_str()),
        _ => None,
    })
}

#[derive(Debug, Clone, Copy)]
struct Ranked {
    score: f32,
    product_id: u32,
    node: usize,
}

/// Maior score primeiro; empate decidido pelo menor id, para o resultado ser determinístico.
impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.total_cmp(&other.score).then_with(|| other.product_id.cmp(&self.product_id))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Ranked {}

/// Top-k com heap mínimo de tamanho k: O(n log k) de tempo e O(k) de memória,
/// em vez de ordenar todos os n candidatos (O(n log n)).
fn top_k(candidates: impl Iterator<Item = Ranked>, k: usize) -> Vec<Ranked> {
    if k == 0 {
        return Vec::new();
    }
    let mut heap = BinaryHeap::with_capacity(k + 1);
    for candidate in candidates {
        heap.push(Reverse(candidate));
        if heap.len() > k {
            heap.pop();
        }
    }
    let mut best: Vec<Ranked> = heap.into_iter().map(|Reverse(ranked)| ranked).collect();
    best.sort_unstable_by(|a, b| b.cmp(a));
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Purchase, Store};

    /// Produtos: 1 Notebook, 2 Fone, 3 Mouse, 7 Teclado (Eletrônicos); 4 Romance, 5 HQ (Livros);
    /// 6 Cadeira (Móveis). Ana: 1, 2. Bruno: 1, 2, 3. Carla: 2, 4. Davi: nada. Eva: 6.
    fn graph() -> Graph {
        let mut store = Store::default();
        for (name, category) in [
            ("Notebook", "Eletrônicos"),
            ("Fone", "Eletrônicos"),
            ("Mouse", "Eletrônicos"),
            ("Romance", "Livros"),
            ("HQ", "Livros"),
            ("Cadeira", "Móveis"),
            ("Teclado", "Eletrônicos"),
        ] {
            store.add_product(name, category, 10.0).unwrap();
        }
        for name in ["Ana", "Bruno", "Carla", "Davi", "Eva"] {
            store.add_customer(name).unwrap();
        }
        for (customer_id, product_id) in [(1, 1), (1, 2), (2, 1), (2, 2), (2, 3), (3, 2), (3, 4), (5, 6)] {
            store.add_purchase(Purchase { customer_id, product_id, quantity: 1, rating: None }).unwrap();
        }
        Graph::from_store(&store)
    }

    fn ids(outcome: &Outcome) -> Vec<u32> {
        outcome.items.iter().map(|item| item.product_id).collect()
    }

    #[test]
    fn cliente_recebe_produtos_de_clientes_parecidos_sem_os_ja_comprados() {
        let outcome = for_customer(&graph(), 1, 5, 3).unwrap();
        assert_eq!(ids(&outcome), vec![3, 4, 7]);
        // Bruno é alcançado pelo Notebook (1) com a maior contribuição, então o Mouse vem "via" 1.
        assert_eq!(outcome.items[0].reason, Reason::BoughtWith { via: 1 });
        assert_eq!(outcome.items[2].reason, Reason::SameCategory { category: "Eletrônicos".into() });
        assert!(outcome.items[0].score > outcome.items[1].score);
    }

    #[test]
    fn produto_recomenda_comprados_juntos() {
        let outcome = for_product(&graph(), 1, 5, 2).unwrap();
        assert_eq!(ids(&outcome), vec![2, 3, 7]);
        assert_eq!(outcome.items[0].reason, Reason::AlsoBought { shared: 2 });
        assert_eq!(outcome.items[1].reason, Reason::AlsoBought { shared: 1 });
    }

    #[test]
    fn respeita_k_e_nao_repete() {
        let graph = graph();
        assert_eq!(ids(&for_customer(&graph, 1, 1, 3).unwrap()), vec![3]);
        assert!(for_customer(&graph, 1, 0, 3).unwrap().items.is_empty());
        let outcome = for_customer(&graph, 2, 50, 5).unwrap();
        let unique: HashSet<u32> = ids(&outcome).into_iter().collect();
        assert_eq!(unique.len(), outcome.items.len());
        assert!(!unique.contains(&1) && !unique.contains(&2) && !unique.contains(&3));
    }

    #[test]
    fn sem_historico_ou_sem_vizinhos_nao_recomenda_nada() {
        let graph = graph();
        assert!(for_customer(&graph, 4, 5, 3).unwrap().items.is_empty());
        assert!(for_product(&graph, 6, 5, 2).unwrap().items.is_empty());
    }

    #[test]
    fn ids_inexistentes_devolvem_none() {
        let graph = graph();
        assert!(for_customer(&graph, 99, 5, 3).is_none());
        assert!(for_product(&graph, 99, 5, 2).is_none());
    }

    #[test]
    fn profundidade_limita_a_busca() {
        let graph = graph();
        let shallow = for_customer(&graph, 1, 5, 1).unwrap();
        // Nível 1 são só os comprados (excluídos): sobra o preenchimento por categoria.
        assert!(shallow.items.iter().all(|item| matches!(item.reason, Reason::SameCategory { .. })));
        assert!(shallow.visited < for_customer(&graph, 1, 5, 3).unwrap().visited);
    }

    #[test]
    fn top_k_ordena_e_desempata_por_id() {
        let ranked = |score, product_id| Ranked { score, product_id, node: 0 };
        let best = top_k([ranked(1.0, 5), ranked(3.0, 1), ranked(1.0, 2), ranked(0.5, 9)].into_iter(), 3);
        let order: Vec<u32> = best.iter().map(|r| r.product_id).collect();
        assert_eq!(order, vec![1, 2, 5]);
    }
}
