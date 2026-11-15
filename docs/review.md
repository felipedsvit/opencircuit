# Revisão de documentação e implementação

- **Data:** 2026-11-15
- **Commit revisado:** `14a231d` (`main`, merge do M4)
- **Escopo:** documentação normativa e de arquitetura × código implementado
- **Método:** leitura de `ROADMAP.md`, `ARCHITECTURE.md`, `FORMAT.md`, `SIMULATION.md`, `LIBRARY.md`, `EXTENDING.md`, `docs/spec02.md` e dos ADRs, cruzada com os crates e os gates de qualidade.
- **Alterações:** nenhuma alteração de código; este documento é o único artefato desta revisão.

## Veredito

O projeto está em bom estado. Os marcos M0–M4 estão implementados, os critérios de saída têm testes e todos os gates passam. Não foi encontrado requisito marcado como concluído que esteja ausente do código.

O que existe é:

1. documentação histórica que ficou para trás;
2. uma ambiguidade contratual no snapshot depois de `reset_fault()`;
3. oportunidades de performance já mapeadas nos próprios benchmarks.

Não foi encontrado bug de corretude que quebre a suíte atual. O risco maior é contratual (semântica do snapshot) e de desempenho em documentos grandes.

## O que foi verificado

Comandos executados nesta revisão:

| Verificação | Resultado |
|---|---|
| `cargo test --workspace --locked` | **686 testes passaram**, 0 falhas |
| `cargo fmt --all -- --check` | passou |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passou |
| `cargo run -p xtask -- check-architecture` | 9 módulos de `circuit-sim` e 57 arquivos do núcleo verificados |
| `cargo run -p xtask -- generate-examples --check` | 31 arquivos, 0 escritos |
| `cargo deny check` | advisories, bans, licenses e sources OK |
| `cargo doc --workspace --no-deps` | **1 warning** (ver “Qualidade Rust”) |

Os crate roots do núcleo têm `#![forbid(unsafe_code)]` e negam `unwrap`, `expect`, `panic`, `todo`, `unimplemented` e `unreachable`. Não foram encontrados esses padrões fora de testes.

O benchmark não foi reexecutado nesta revisão; os números de desempenho citados abaixo vêm de [`benchmarks.md`](benchmarks.md).

## O que foi adiado ou está fora de escopo

Nada disso parece ter sido esquecido: está registrado no roadmap, na spec ou nos ADRs.

| Item | Situação |
|---|---|
| `cargo-fuzz` | Adiado na spec §19.2: exige nightly, cria `Cargo.lock` aninhado e `libfuzzer-sys` usa licença NCSA. O loader, o compilador, as transações e o Modbus têm proptests. |
| Plugins com código/WASM | Fora de escopo. O M4 entregou **bibliotecas só de dados** (ADR-0008). O checklist do M4 diz “Plugins”, mas o `ROADMAP.md` esclarece “data-only”. |
| OPC UA e MQTT | Avaliados no ADR-0010, não implementados. |
| Cliente Modbus / controle real | Fora de escopo; existe apenas o servidor Modbus para teste. |
| Simulação analógica, tensão/corrente/impedância, transitórios, pneumática, hidráulica, PLC real, colaboração, importação de CAD proprietário, certificação | Fora do MVP. |
| `circuit-topology` | Não foi separado de `circuit-sim`; é um gatilho futuro (§5.3). |
| Migrações de schema | Tabela vazia porque o schema atual é v1; o driver e os testes com migrações sintéticas existem. |
| `assets/` e `ATTRIBUTION.md` | `assets/` só tem `.gitkeep`; sem asset, não há atribuição a registrar. |
| Build WASM | Compilado e verificado no CI, mas não publicado. |

## Divergências entre documentação e código

### DOC-01 — `AGENTS.md` desatualizado sobre `clippy.toml` (baixa)

**Onde:** `AGENTS.md:53`

Diz: “não há `rustfmt.toml` ou `clippy.toml` ainda”.

Na prática, existe `clippy.toml` na raiz e em cada crate do núcleo, com `disallowed-types`. O `rustfmt.toml` realmente não existe. É uma frase fácil de corrigir.

### DOC-02 — Comando `cargo xtask` na spec (baixa)

**Onde:** `docs/spec02.md:1461`

A spec mostra:

```text
cargo xtask check-architecture
```

Não existe binário `cargo-xtask` no workspace. O comando real, usado no CI e no README, é:

```sh
cargo run --locked -p xtask -- check-architecture
```

### DOC-03 — `disallowed-methods` que não existem (baixa)

**Onde:** `docs/spec02.md:294`

A spec diz que cada `clippy.toml` tem `disallowed-types` **e** `disallowed-methods` para I/O. Os arquivos reais só têm `disallowed-types` (`Instant`, `SystemTime`, `HashMap`, `HashSet`, `File`, `TcpStream`, `UdpSocket`). A proteção contra `std::fs` etc. vem da análise textual do `xtask`, não de `disallowed-methods`.

### DOC-04 — Comentário de `NonOrthogonalWire` desatualizado (baixa)

**Onde:** `crates/circuit-model/src/diagnostics.rs:125-127`

O comentário diz:

> “Reserved: emitted once symbol geometry exists.”

Mas o diagnóstico já é emitido em `crates/circuit-sim/src/compile.rs:83-102`, e há testes cobrindo `OC-STR-009`. A spec (`docs/spec02.md:498,1145`) e o `FORMAT.md:173` já descrevem o comportamento correto. Só o comentário ficou para trás.

### DOC-05 — Spec §5 ainda descreve o workspace do M1 (baixa/média)

**Onde:** `docs/spec02.md:209-228, 319-325`

A estrutura inicial e o `Cargo.toml` de referência listam apenas:

- `apps/cli`
- `circuit-model`
- `circuit-project`
- `circuit-sim`
- `xtask`

O workspace atual tem 11 membros, incluindo `circuit-renderer`, `circuit-commands`, `circuit-editor`, `circuit-export`, `circuit-io` e `apps/desktop`. A spec avisa que os blocos Rust são ilustrativos (§22), mas um leitor pode copiar a lista achando que é o estado atual.

### DOC-06 — `ParamValue::Enum` não existe (baixa)

**Onde:** `docs/spec02.md:412`

A spec ilustra:

```rust
pub enum ParamValue { Bool(bool), Int(i64), Enum(String), Text(String) }
```

O código tem apenas `Bool`, `Int` e `Text` (`crates/circuit-model/src/schematic.rs:87-94`). Parâmetros enumerados usam `ParamType::Enum` para validar um `Text`. O comportamento é equivalente, mas o tipo mostrado está errado.

### DOC-07 — Status dos marcos e checklist na spec (baixa)

**Onde:** `docs/spec02.md:1540-1607` e Apêndice A (`1763-1781`)

O M0 aparece como “(concluído)”; M1, M2a, M2b, M3 e M4 não. O Apêndice A tem todos os itens como `[ ]`, embora o `ROADMAP.md` marque M0–M4 como `done` e os testes de saída existam. O roadmap é a fonte operacional, mas a spec dá a impressão de que os marcos ainda estão pendentes.

### DOC-08 — `ARCHITECTURE.md` com linguagem de M1 (baixa)

**Onde:** `ARCHITECTURE.md:18, 46-48`

Diz “The M1 workspace is deliberately small” e “M2 adds…, M3 adds…, M4 adds…”. A tabela de crates logo abaixo já está atualizada; é só uma questão de redação histórica.

### DOC-09 — Semântica de `faulted` no snapshot após `reset_fault()` (média)

**Onde:** `SIMULATION.md:265-275`, `crates/circuit-model/src/snapshot.rs:99-102`, `crates/circuit-sim/src/runtime.rs:748-749, 804-815`

O `SIMULATION.md` diz que `snapshot()` descreve o último tick resolvido e lista `faulted` e `faults` como parte desse retrato.

O código faz:

```rust
faulted: self.state.faulted,      // estado atual da trava
faults: self.view.faults.clone(), // falhas do último tick resolvido
```

Depois de `reset_fault()`:

- `state.faulted` vira `false`;
- `view.faults` continua com a falha do tick anterior.

Ou seja, é possível obter um snapshot com `faulted = false` e `faults = [OC-ELEC-001]`. O desktop contorna isso manualmente em `apps/desktop/src/session.rs:487-489`:

```rust
if let Some(snapshot) = &mut self.snapshot {
    snapshot.faulted = false;
}
```

Isso é um sinal de contrato ambíguo: ou `faulted` significa “travado agora”, ou significa “houve falha no tick descrito”. As duas coisas não podem ser verdadeiras ao mesmo tempo nesse snapshot.

**Recomendação:** decidir explicitamente. A opção mais limpa é guardar `faulted` no `TickView` (como `faults`) e expor a trava atual separadamente, por exemplo com `is_faulted()`. Depois, adicionar um teste de regressão para `reset_fault()` + `snapshot()`.

## Achados de qualidade Rust

### RUST-01 — `cargo doc` falha com `-D warnings` (baixa)

**Onde:** `crates/circuit-sim/src/compiled.rs:3`

O link `[`crate::compile`]` é ambíguo porque existe tanto a função quanto o módulo `compile`. O rustdoc sugere:

```rust
[`crate::compile()`]
```

ou

```rust
[`mod@crate::compile`]
```

O `cargo doc` normal gera só esse warning; com `RUSTDOCFLAGS="-D warnings"` ele falha. O CI não roda rustdoc, então passou despercebido.

### RUST-02 — `DeviceId(0)` como sentinela em caminhos internos (média)

**Onde:** `crates/circuit-sim/src/solve.rs:309` e `crates/circuit-sim/src/runtime.rs:503`

Os dois trechos usam:

```rust
.map_or(DeviceId(0), |device| device.id)
```

Mas o contrato do projeto diz que IDs começam em 1 e zero não é um ID válido (`ids.rs:4-5`, `FORMAT.md:12-14`). Hoje isso só aconteceria se o circuito compilado estivesse inconsistente, mas o fallback silencioso pode gerar um diagnóstico ou evento de trace com `DeviceId(0)`, violando a invariante em vez de falhar de forma estruturada.

**Recomendação:** usar `if let Some(device)` / `filter_map` e simplesmente não emitir o evento, ou retornar um erro interno. Como é caminho “impossível”, um `debug_assert!` seria aceitável, mas não um ID inválido no domínio.

### RUST-03 — Validação dupla por transação (média)

**Onde:** `crates/circuit-commands/src/transaction.rs:102-114`

`Transaction::apply_to` faz:

1. `doc.validate()` para calcular os erros de integridade anteriores;
2. clone do documento;
3. aplica as edições;
4. `limits::check`;
5. `draft.validate()` novamente para comparar os erros.

Isso é correto, mas caro. O `Editor::preview` chama `apply_to` **a cada frame de arraste** (`crates/circuit-editor/src/editor.rs:467-478`), e o `App` monta a cena com esse preview todo frame (`apps/desktop/src/app.rs:925-934`). O próprio [`benchmarks.md`](benchmarks.md) já aponta “manter os erros de integridade do documento atual em vez de recalculá-los a cada transação” como próxima otimização.

### RUST-04 — Clones em caminhos de exportação/serialização (baixa)

**Onde:** `crates/circuit-project/src/lib.rs:154-160`

`to_string` clona o documento inteiro para ordenar canonicamente antes de serializar. Para salvar/exportar não é crítico, mas é fácil de evitar com uma ordenação sobre uma visão emprestada ou com uma cópia de trabalho reutilizada.

### RUST-05 — Módulos grandes (baixa)

`catalog.rs` tem cerca de 2.178 linhas, `compile.rs` cerca de 1.360, `xtask/examples.rs` cerca de 1.301 e `apps/desktop/src/app.rs` cerca de 1.149. Os testes são extensos e passam, então não é um problema imediato; se esses arquivos continuarem crescendo, vale separar por responsabilidade.

## O que pode ser otimizado

Os números abaixo vêm de [`benchmarks.md`](benchmarks.md). Eles são a melhor evidência disponível para priorizar; o benchmark não foi reexecutado nesta revisão.

### OPT-01 — Arraste/edição: evitar clone + validação dupla por frame (alta)

**Evidência:** “move edit” custa aproximadamente:

| Documento | Custo |
|---:|---:|
| 2.000 dispositivos | 7,8 ms |
| 5.000 dispositivos | 24,8 ms |
| 10.000 dispositivos | 54,8 ms |

Some a cena (`2,6 ms` a 2k; `6,9 ms` a 5k; `15,3 ms` a 10k) e fica claro por que arrastar em 5k/10k não é fluido. O `Editor::preview` faz esse trabalho a cada frame.

**Ideias:**

- cachear os erros de integridade do documento base na `Session`;
- criar um caminho `apply_preview` que aplica a transação sem `integrity_errors(doc)` e sem `sort_canonical`, validando só no commit;
- ou, para `move_by`, transladar diretamente os itens da cena durante o gesto e aplicar a transação apenas no `pointer_up`.

O terceiro caminho é o mais ambicioso, mas também o que mais reduz trabalho por frame.

### OPT-02 — Cena: cachear geometria estática e recortar antes de construir (alta)

**Evidência:** `build_scene` custa cerca de `2,2 ms` em 2k, `5,9 ms` em 5k e `13,4 ms` em 10k em repouso; em execução, `2,6 ms`, `6,9 ms` e `15,3 ms`.

Hoje `build_scene` (`crates/circuit-renderer/src/build.rs:102-142`) reconstrói:

- `LayoutIndex`;
- dois `BTreeMap` de `by_id`;
- mapas de estado (`wires`, `junctions`, `motors`, `timers`);
- todas as formas e labels.

O `canvas` já faz culling no momento de desenhar (`apps/desktop/src/canvas.rs:84-90`), mas a cena inteira já foi construída antes. Para documentos grandes, o recorte chega tarde.

**Ideias:**

- separar a cena “estática” (geometria, labels) da parte dependente do snapshot (cores de fio, contatos, bobinas);
- cachear a parte estática por revisão do documento;
- passar a área visível para `build_scene` e evitar construir o que está fora dela;
- cachear o `LayoutIndex` junto com a cena.

### OPT-03 — Solver: reutilizar buffers por solve (média)

**Evidência:** no pior caso (osciladores, máscara muda a cada tick), o custo é de `245 µs/tick` em 2k e `1,4 ms/tick` em 10k.

`Solver::solve` já reaproveita `groups`, `dead`, `faulted`, `sections` e `solution`, mas ainda aloca por solve:

- `feeds: BTreeMap<u32, (GroupFeed, Vec<DeviceId>)>` (`solve.rs:273`);
- `faults: Vec<Diagnostic>` (`solve.rs:285`);
- `components: BTreeMap<u32, DeadComponent>` (`solve.rs:603`);
- um `Vec<(u32, ConductorSet, bool)>` por carga em `find_series_loads` (`solve.rs:605`).

**Ideias:**

- transformar `feeds` e `components` em `Vec` indexados pelo grupo (o número de grupos é limitado por `net_count`), preservando a ordem determinística;
- manter buffers de `faults`, `states` e `DeadComponent` no `Solver` e fazer `clear()`/reuso;
- continuar usando a memoização por `last_mask`, que já evita o custo quando o circuito está estável.

### OPT-04 — `snapshot()`: uma passada em vez de várias (baixa)

**Evidência:** `23 µs` em 2k, `57 µs` em 5k e `113 µs` em 10k. Não é urgente, mas é chamado uma vez por frame.

Hoje o snapshot varre:

- sinais três vezes (`CoilActive`/`TimerOut`/`Powered`, `Actuated`, `Tripped`);
- switches três vezes (duas para `owners_by_level`, uma para `closed_symbols`);
- cargas duas vezes (energizadas e motores);
- dispositivos uma vez (timers).

**Ideias:** uma passada única por coleção, preenchendo os vetores; ou reutilizar vetores internos se a API permitir. O ganho é menor que o da cena e do solver.

### OPT-05 — `LayoutIndex` para hit-test e `control_of` (baixa/média)

**Evidência:** `symbol_at` custa `274 µs` em 2k e `1,57 ms` em 10k. É por clique, então é tolerável, mas `LayoutIndex::new` também é reconstruído em `Session::control_of` e em outros caminhos.

**Ideia:** cachear o `LayoutIndex` na `Session`, invalidando-o quando o documento mudar. Isso beneficia clique, seleção e a própria construção da cena.

### OPT-06 — Serialização canônica sem clone (baixa)

**Evidência:** `to_string` clona o documento inteiro (`circuit-project/src/lib.rs:156`). O custo é relevante só em salvar/exportar, não por frame.

**Ideia:** ordenar sobre uma visão emprestada ou manter uma cópia de trabalho reutilizável. Baixa prioridade.

### OPT-07 — Gate de rustdoc no CI (qualidade)

**Evidência:** `cargo doc` com `-D warnings` falha por um link ambíguo em `compiled.rs:3`.

**Ideia:** corrigir o link e, em um PR separado de CI (conforme o `AGENTS.md`), adicionar `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` ao pipeline. Isso evita que novos links quebrados ou docs ambíguas passem.

## Ordem sugerida

1. **PR só de documentação:** corrigir `AGENTS.md:53`, o comando `cargo xtask`, `disallowed-methods`, o comentário de `NonOrthogonalWire`, a lista de membros da spec §5 e os status §20/Apêndice A.
2. **PR de contrato:** resolver a semântica de `faulted` × `faults` após `reset_fault()`, com teste de regressão.
3. **PR de performance de edição:** cachear a linha de base de integridade e evitar validação dupla no preview de arraste; medir com o bench existente.
4. **PR de performance de cena:** cachear geometria estática/`LayoutIndex` e recortar antes de construir a cena.
5. **PR de performance do solver:** reutilizar buffers de `feeds`, `faults`, `components` e `states`.

Não alterar `.github/workflows/` nos PRs de feature ou correção; o gate de rustdoc, se adotado, pede um PR próprio.
