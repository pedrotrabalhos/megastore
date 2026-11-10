# ConectaStore — recomendação de produtos baseada em grafos

Trabalho final da disciplina **Data Structure Strategy and Implementation** (UniFECAF).
Autor: **Pedro Henrique Gadelha do Nascimento**.

## Objetivo

A MegaStore recomenda só os mais vendidos ou produtos da mesma categoria. O ConectaStore modela clientes, produtos e categorias como um **grafo** e percorre as conexões de compra para recomendar o que clientes com comportamento parecido compraram. O sistema recomenda a partir de um **cliente** ("recomendado para você") ou de um **produto** ("quem comprou também comprou").

## Tecnologias e estruturas

- **Rust** (edition 2024). Dependências: `clap` (CLI), `serde` e `serde_json` (persistência). O grafo foi implementado à mão, sem bibliotecas de grafos.

| Estrutura | Uso | Por quê |
|---|---|---|
| Lista de adjacência `Vec<Vec<Edge>>` | arestas do grafo | memória O(V + E); o grafo é esparso (matriz seria O(V²)) |
| `HashMap<Node, usize>` | id do domínio → índice do vértice | acesso O(1) médio |
| `HashMap<u32, usize>` | consulta de produto/cliente por id | O(1) contra O(n) da busca linear |
| `VecDeque` | fila da BFS | inserção e remoção O(1) nas pontas |
| `HashMap<usize, Visit>` | vértices visitados, nível e score | cada vértice entra na fila uma única vez |
| `HashSet` | produtos já comprados e já escolhidos | evita recomendações repetidas ou já compradas |
| `BinaryHeap` (heap mínimo) | top-k | O(n log k) em vez de ordenar tudo |

## Modelagem

- **Vértices:** cliente, produto e categoria.
- **Arestas:**
  - cliente–produto (compra), com peso = quantidade × nota ÷ 3; sem nota, o fator é 1. Compras repetidas somam peso.
  - produto–categoria, com peso 1.
- **Tipo:** heterogêneo, não-direcionado, ponderado e esparso.

## Algoritmo de recomendação

1. **BFS com profundidade limitada.** A partir de um cliente: cliente → produtos comprados → outros compradores → produtos desses compradores (profundidade 3). A partir de um produto: produto → compradores → outros produtos (profundidade 2). Vértices de categoria ficam fora da busca.
2. **Score.** É a probabilidade de um passeio aleatório, que segue as arestas na proporção do peso, chegar ao produto. Como a BFS fecha um nível antes de abrir o próximo, cada vértice já tem todas as contribuições somadas quando sai da fila.
3. **Penalidade de popularidade.** O score é dividido por √(compradores), para não recomendar sempre os mais vendidos.
4. **Sem duplicatas.** Produtos já comprados são excluídos com um `HashSet`. O top-k vem de um heap.
5. **Preenchimento por categoria.** Se faltarem itens, entram os produtos mais comprados das mesmas categorias.
6. **Motivo.** Cada item informa por que foi recomendado (ex.: "porque você comprou X").

**Complexidade:**
- Recomendação: O(S + C log k), onde S é a soma dos graus dos vértices alcançados até a profundidade d e C é o número de candidatos. Não depende do tamanho total do catálogo.
- Construção do grafo: O(V + E log E). A ordenação existe para garantir resultados determinísticos.
- Memória: O(V + E).

## Compilar e executar

Requer Rust 1.88 ou mais recente.

```sh
cargo build --release
cargo run --release -- --help
```

Os dados ficam em `megastore.json`, no diretório atual (`--data <arquivo>` muda o caminho). Se o arquivo não existir, uma base de exemplo é gerada automaticamente.

## Testes

```sh
cargo test
```

- **Unitários**, em `src/`: grafo, catálogo, gerador de dados e recomendação, sobre um grafo pequeno montado à mão.
- **Integração**, em `tests/`:
  - `recommendation.rs` checa, numa base de 2.000 produtos, que as recomendações são únicas, inéditas, ordenadas e determinísticas;
  - `cli.rs` executa o binário de verdade (cadastro, compra, recomendação, erros e benchmark).

## Exemplos de uso

```sh
megastore seed                                   # base de exemplo: 48 produtos, 20 clientes
megastore seed --products 5000 --customers 1000  # base maior
megastore product list --category livros
megastore product show 1                         # detalhes + "quem comprou também comprou"
megastore product add --name "Mouse Gamer" --category Eletrônicos --price 149.90
megastore product categories
megastore customer list
megastore customer show 3                        # histórico + "recomendado para você"
megastore customer add --name "Pedro"
megastore purchase add 21 2 --rating 5           # cria a aresta cliente–produto
megastore purchase remove 21 2                   # remove a aresta
megastore recommend customer 3 -k 5 --depth 3
megastore recommend product 1 -k 5
megastore stats                                  # vértices, arestas, memória lista × matriz
megastore bench                                  # tempos com 1k a 1M produtos
```

Durante o desenvolvimento, troque `megastore` por `cargo run --release --`.

**Entradas e limites:**
- A busca e as categorias ignoram maiúsculas e acentos (`--category eletronicos` encontra "Eletrônicos").
- O preço aceita vírgula (`--price 149,90`).
- Limites de valores: `-k` de 1 a 100; `--depth` de 3 a 9 para cliente e de 2 a 8 para produto; quantidade de 1 a 1000; nota de 1 a 5; nomes de até 120 caracteres.
- Valores fora dessas faixas encerram com erro (código 1 ou 2), sem pânico.

```
$ megastore recommend customer 3
Recomendações para Carla Silva (cliente 3), BFS com profundidade 3

#  ID  Produto                      Categoria  Score   Motivo
─  ──  ───────────────────────────  ─────────  ──────  ────────────────────────────────────────────
1  10  Calça Jeans Básico           Vestuário  0,0375  porque você comprou Livro de Receitas Básico
2  26  Tênis Básico                 Vestuário  0,0172  porque você comprou Livro de Receitas Básico
3  2   Camiseta Básico              Vestuário  0,0058  porque você comprou Livro de Receitas Básico
4  34  Moletom Básico               Vestuário  0,0046  porque você comprou Livro de Receitas Básico
5  36  Livro de Programação Básico  Livros     0,0037  porque você comprou Livro de Receitas Básico

BFS visitou 13 vértices em 5,1 µs.
```

## Arquitetura

```
src/
├── main.rs       ponto de entrada: chama cli::run e trata erros
├── lib.rs        módulos públicos (usados pelos testes de integração)
├── cli.rs        subcomandos (clap) e formatação da saída
├── store.rs      catálogo: produtos, clientes, compras, índices HashMap, JSON
├── graph.rs      grafo em lista de adjacência
├── recommend.rs  BFS, score, top-k e preenchimento por categoria
├── seed.rs       gerador determinístico de dados sintéticos
├── bench.rs      medições de tempo
└── error.rs      tipo de erro
tests/
├── recommendation.rs
└── cli.rs
```

Cada comando carrega o JSON, monta o grafo em memória, executa e, se alterou dados, salva o JSON. O arquivo guarda os dados e o grafo é a estrutura usada para percorrê-los. A gravação é atômica: o programa grava num arquivo temporário e renomeia. Ao carregar, o arquivo passa pelas mesmas validações do cadastro. O sistema não tem trava entre processos: dois comandos que alterem os dados ao mesmo tempo podem perder uma das alterações, o que é aceitável para uma CLI de uso individual.

## Desempenho

`cargo run --release -- bench`: dados sintéticos com clientes = produtos ÷ 5 e cerca de 8 compras por cliente. Os tempos são médias de 200 operações aleatórias (Linux, build release).

| Produtos | Vértices | Arestas | Lista adj. | Matriz adj. (estimada) |
|---:|---:|---:|---:|---:|
| 1.000 | 1.208 | 2.709 | 179 KB | 5,6 MB |
| 10.000 | 12.008 | 26.882 | 1,7 MB | 550 MB |
| 100.000 | 120.008 | 270.103 | 17,4 MB | 53,7 GB |
| 1.000.000 | 1.200.008 | 2.699.583 | 173,9 MB | 5,2 TB |

| Produtos | Construção | Busca HashMap | Busca linear | Rec. cliente | Rec. produto |
|---:|---:|---:|---:|---:|---:|
| 1.000 | 0,3 ms | 10 ns | 174 ns | 23 µs | 4 µs |
| 10.000 | 4,2 ms | 61 ns | 2,2 µs | 38 µs | 21 µs |
| 100.000 | 77 ms | 113 ns | 24 µs | 137 µs | 383 µs |
| 1.000.000 | 1,17 s | 171 ns | 525 µs | 0,8 ms | 7,0 ms |

Leitura dos resultados:
- A matriz de adjacência fica inviável a partir de ~100 mil produtos.
- A busca por `HashMap` permanece quase constante, enquanto a busca linear cresce com n.
- A recomendação por cliente cresce bem menos que o catálogo, porque depende só da vizinhança visitada.
- A recomendação por produto fica mais lenta no maior volume por causa do preenchimento por categoria, que percorre a categoria inteira quando o produto tem poucos compradores. A melhoria natural é guardar previamente os produtos mais populares de cada categoria.
