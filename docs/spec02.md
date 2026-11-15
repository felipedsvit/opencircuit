# OpenCircuit

## Documento Técnico Consolidado e Prospectivo

| Campo | Valor |
|---|---|
| Nome de trabalho | OpenCircuit |
| Versão do documento | 0.3 — proposta consolidada e pragmática |
| Data | 13/11/2026 |
| Status | Arquitetura proposta; contratos fechados para prototipagem; sujeita a validação por protótipos e testes |
| Plataforma inicial | Desktop — Linux, Windows e macOS |
| Plataforma futura | WebAssembly |
| Linguagem | Rust |
| Interface | egui/eframe |
| Licença pretendida | MIT OR Apache-2.0 |

---

## 0. Tese e escopo do documento

O circuito é um modelo de domínio independente da interface, compilado para uma representação executável e simulado com tempo virtual e semântica determinística. Este documento fixa:

1. **Contratos consistentes entre seções** (assinatura do compilador, tipos de sinais, snapshot, catálogo).
2. **Estrutura inicial mínima:** `circuit-model`, `circuit-sim`, `circuit-project`, `apps/cli` e `xtask`; divisão em crates apenas por gatilho objetivo (§5.3).
3. **Semântica fechada** para o que define o formato de arquivo e os testes golden: modelo de rede, tick, temporizadores, entradas, falhas e persistência.

### 0.1 Como ler este documento

- Os blocos Rust são **ilustrativos**. Quando o código existir, os tipos reais e os testes prevalecem; o documento é corrigido na mesma PR que alterar um contrato.
- `SIMULATION.md` e `FORMAT.md` (§22) são **normativos** e derivam deste documento. Este documento é a visão e a justificativa.
- Itens são marcados como **[decidido]**, **[a validar no protótipo]** ou **[adiado]** quando o status não for óbvio pelo contexto.

---

## 1. Resumo executivo

OpenCircuit é uma proposta de editor e simulador de esquemas elétricos de comando, com foco inicial em automação e painéis industriais. O objetivo é oferecer uma alternativa open source a ferramentas como o CADe SIMU, começando por circuitos de comando e evoluindo de forma incremental para circuitos de potência, documentação técnica, integração industrial e extensibilidade.

A arquitetura separa o documento elétrico, o catálogo de dispositivos, a compilação da topologia, o solver discreto, a lógica de dispositivos, o runtime de simulação, a edição gráfica, a renderização e a persistência. A decisão central permanece:

> **Princípio arquitetural:** OpenCircuit não deve ser um editor de desenhos que tenta inferir eletricidade a partir de pixels. Deve ser um modelo de circuito que possui uma representação gráfica.

O fluxo é: o editor altera o modelo; o compilador o transforma em uma representação executável; o simulador calcula estados em ticks de tempo virtual; a renderização apresenta o documento e o resultado.

O MVP é uma partida direta de motor — circuito de selo, contatos auxiliares, proteção térmica e sinalização — **primeiro executada e testada sem interface** (CLI + golden), depois visualizada (viewer read-only) e só então editável.

**A prioridade não é desenhar muitos símbolos. É provar que um circuito pequeno possui semântica correta, reproduzível e testável, e entregar cedo algo que um usuário possa ver funcionando.**

---

## 2. Objetivos e escopo

### 2.1 Público-alvo e posicionamento

- **Público primário:** estudantes e instrutores de eletrotécnica e automação; técnicos que documentam e testam lógica de comando; quem quer versionar esquemas em Git e rodá-los em CI.
- **Referência de mercado:** o CADe SIMU, ferramenta fechada e voltada a Windows (verificar licença e plataformas atuais antes de qualquer comparação pública).
- **Diferenciais pretendidos:** open source; Linux/macOS/Windows; formato aberto, textual e diffável; testes automatizados de lógica de comando; extensibilidade.
- **Não são diferenciais no MVP:** biblioteca ampla de símbolos, pneumática, PLC. Estão fora do escopo (§2.5).
- **Consequência de projeto:** a adoção será decidida pela experiência do editor e pela qualidade dos símbolos, não pela CLI. Por isso o viewer read-only (M2a) e o editor (M2b) não ficam para "depois do núcleo perfeito"; o núcleo headless existe para que a GUI nunca seja dona da lógica elétrica.

### 2.2 Objetivos

1. Criar um editor de esquemas elétricos industriais.
2. Simular circuitos de comando de forma determinística e reproduzível.
3. Diagnosticar erros de ligação e de configuração com códigos estáveis.
4. Manter o núcleo independente de interface gráfica e de sistema operacional.
5. Permitir automação de testes por CLI.
6. Disponibilizar um formato de projeto documentado e versionado.
7. Manter o projeto aberto a contribuições, extensões e integrações futuras.

### 2.3 Plataformas

| Plataforma | Fase | Observação |
|---|---|---|
| Linux | MVP | Aplicação desktop nativa |
| Windows | MVP | Aplicação desktop nativa |
| macOS | MVP | Aplicação desktop nativa |
| WebAssembly | Posterior | Após validação de dependências e experiência de uso |

A compatibilidade WASM é requisito de portabilidade do **núcleo** (`circuit-model`, `circuit-sim`, `circuit-project`) desde o início, verificada em CI, mas não compromisso de entregar a aplicação web no MVP.

### 2.4 Dentro do MVP

- Fontes de alimentação (DC de comando; monofásica e trifásica para o circuito de força) e referências de condutores.
- Botões NA e NF; **chave com retenção** (`LatchingSwitch`: seletora, disjuntor, botão de emergência com trava).
- Contatos auxiliares NA e NF; contatos principais de contator.
- Bobinas de contatores e relés.
- Temporizadores TON e TOF.
- Relé térmico (contatos auxiliares e disparo manual na simulação).
- **Fusível** como elemento passivo (sem simulação de fusão).
- Lâmpadas e buzzer.
- Motor trifásico simplificado como carga (energizado e sentido de rotação; sem grandezas físicas).
- Editor com seleção, inserção, movimentação, rotação e exclusão.
- Fios, junções e roteamento ortogonal básico.
- Snap-to-grid, zoom e pan.
- Undo e redo.
- Salvar e carregar projeto.
- Simulação interativa.
- Diagnósticos estruturais, semânticos e elétricos básicos.
- CLI headless para executar cenários e testes golden.

### 2.5 Fora do MVP

- Simulação analógica geral; cálculo de tensão, corrente, impedância e transitórios.
- Pneumática e hidráulica.
- PLC real e execução de programas Ladder completos.
- Modbus, OPC UA e MQTT (M4: servidor Modbus TCP, [ADR-0009](adr/0009-industrial-io.md); OPC UA e MQTT avaliados, [ADR-0010](adr/0010-opcua-mqtt-evaluation.md)).
- Plugins com código (WASM) e catálogos externos (M4: bibliotecas só de dados, [ADR-0008](adr/0008-extension-libraries.md); plugins com código fora do escopo).
- Colaboração em tempo real.
- Importação de arquivos proprietários do CADe SIMU.
- Certificação ou validação normativa de instalações reais.

### 2.6 Limite de responsabilidade

O OpenCircuit será uma ferramenta de desenho, aprendizado e simulação preliminar. Seus resultados não substituem projeto elétrico profissional, análise de risco, ensaios, comissionamento, validação de proteção ou atendimento às normas aplicáveis. O software não deverá alegar certificar instalações ou garantir segurança operacional.

---

## 3. Princípios arquiteturais

1. **Domínio independente:** nenhum crate do núcleo depende de egui, eframe ou APIs de sistema operacional.
2. **Modelo explícito:** dispositivos, símbolos, terminais, fios, junções e fontes são entidades distintas.
3. **Compilação do circuito:** o documento é compilado para uma representação executável antes da simulação.
4. **Determinismo:** com o mesmo circuito compilado, configuração, estado inicial e sequência de entradas, o runtime produz o mesmo trace. O núcleo usa apenas inteiros e booleanos; não há ponto flutuante no formato de arquivo, no compilador, no solver nem no runtime.
5. **Tempo virtual:** o motor não lê o relógio do sistema.
6. **Snapshot coerente:** cada snapshot representa um único tick resolvido.
7. **Persistência versionada:** o formato tem `schema_version`, migrações testadas e política explícita para versões desconhecidas.
8. **Diagnóstico estruturado:** erros têm códigos estáveis (`OC-<CAT>-NNN`, por exemplo `OC-ELEC-001`) e referências aos elementos envolvidos.
9. **Testabilidade headless:** toda a simulação roda sem GUI.
10. **Evolução incremental:** o MVP não simula grandezas físicas que o solver não calcula; casos fora do modelo são rejeitados com diagnóstico, nunca "aproximados".
11. **Módulos antes de crates:** fronteiras nascem como módulos com regras de import verificadas; viram crates quando um gatilho objetivo for atingido (§5.3).
12. **Código como fonte de verdade dos contratos:** o documento justifica; os tipos e os testes definem.

---

## 4. Arquitetura geral

### 4.1 Fluxo de dados

```text
 .ocircuit (JSON)
      │  circuit-project: from_str / to_string / migrate
      ▼
 Schematic ─────────────── ComponentCatalog              (circuit-model)
      │                          │
      └────────────┬─────────────┘
                   ▼  sim::compile
        CompileOutput { circuit: Option<_>, diagnostics } (circuit-sim)
                   │
                   ▼
        Simulator::new(circuit, SimConfig)
                   │  step / step_n(inputs)
                   │     ├─ solve(circuit, switches_fechados) → ElectricalSolution
                   │     └─ logic::evaluate(...)              → próximos sinais
                   ├──► TraceEvent (log de mudanças)
                   └──► snapshot() → SimSnapshot            (tipo em circuit-model)
                                         │
                  ┌──────────────────────┴──────────────┐
                  ▼                                     ▼
       renderer (M2a): build_scene → Scene        CLI: trace / JSON
                  ▼
       egui (apps/desktop) / SVG / PDF
```

O diagrama é conceitual. As dependências entre crates e módulos seguem a §5.4.

### 4.2 Separação de responsabilidades

- **Documento (`circuit-model`):** descreve o circuito e sua geometria lógica. Não sabe simular.
- **Catálogo (`circuit-model::catalog`):** modelos de componentes, terminais, parâmetros, símbolos e limites de instâncias.
- **Compilação (`circuit-sim::compile`):** valida o documento, resolve terminais, constrói redes de cobre e emite `CompiledCircuit`.
- **Solver (`circuit-sim::solve`):** dado o conjunto de switches fechados, calcula grupos, condutores presentes, cargas energizadas e falhas. Não conhece sinais nem dispositivos.
- **Lógica (`circuit-sim::logic`):** bobinas, temporizadores, térmico e atraso de atuação; produz os sinais do próximo tick.
- **Runtime (`circuit-sim::runtime`):** orquestra o tick, aplica entradas, mantém estado, emite trace, snapshot e checkpoints.
- **Projeto (`circuit-project`):** serializa, desserializa, valida schema e migra documentos. Não acessa o sistema de arquivos.
- **Editor, comandos e renderer:** entram em M2a/M2b (§5.3); consomem apenas tipos de `circuit-model`.
- **CLI (`apps/cli`):** executa validação, cenários, testes golden e (M3) exportação.

### 4.3 Rede de cobre e grupo: dois níveis de conectividade

O termo "net" tem dois sentidos distintos, separados aqui:

| Conceito | Quando é calculado | Une o quê | Identidade |
|---|---|---|---|
| **Rede de cobre** (`NetIx`) | Uma vez, na compilação | Pinos ligados por fios e junções | Índice canônico (menor `PinIx` do membro) |
| **Grupo** | A cada tick (e memoizado) | Redes de cobre unidas por switches fechados | Índice canônico (menor `NetIx` do membro) |

A rede de cobre depende só da geometria lógica do documento. O grupo depende também do estado dos contatos, e só o grupo carrega o conjunto de condutores presentes (§9).

---

## 5. Workspace Rust

### 5.1 Estrutura inicial

A árvore e o `Cargo.toml` desta seção são o ponto de partida do M1. O workspace atual tem onze membros (`circuit-model`, `circuit-sim`, `circuit-project`, `circuit-renderer`, `circuit-commands`, `circuit-editor`, `circuit-export`, `circuit-io`, `apps/cli`, `apps/desktop` e `xtask`); a lista vigente está em `AGENTS.md` e em `ARCHITECTURE.md`.

```text
opencircuit/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .gitattributes               # *.ocircuit, *.trace, *.json: text eol=lf
├── deny.toml
├── README.md
├── LICENSE-APACHE
├── LICENSE-MIT
├── CONTRIBUTING.md
├── SIMULATION.md                # normativo: tick, timers, solver, formato de trace
├── FORMAT.md                    # normativo: schema, IDs, chaves, migrações
├── apps/
│   ├── cli/
│   └── desktop/                 # M2a (viewer) / M2b (editor)
├── crates/
│   ├── circuit-model/           # ids, geometria, documento, catálogo, diagnósticos, snapshot
│   ├── circuit-sim/             # compile, solve, logic, runtime, trace
│   └── circuit-project/         # JSON, validação de schema, migrações
├── assets/
│   └── symbols/                 # com proveniência e licença por arquivo
├── examples/
│   ├── direct-starter/          # comando + força, com cenários
│   ├── forward-reverse/
│   └── star-delta/              # referência de requisitos (M3)
├── tests/
│   ├── fixtures/
│   └── integration/
└── xtask/                       # check-architecture, update-golden
```

`circuit-editor`/`circuit-commands`, `circuit-renderer` e `circuit-export` **não existem** no início. Entram quando houver consumidor real (§5.3): `circuit-renderer` no M2a, `circuit-commands` e `circuit-editor` no M2b, `circuit-export` no M3 e `circuit-io` no M4.

### 5.2 Responsabilidade de crates e módulos

| Unidade | Módulos | Responsabilidade |
|---|---|---|
| `circuit-model` | `ids`, `geometry`, `supply`, `schematic`, `validate`, `catalog`, `input`, `diagnostics`, `snapshot`, `builder` | Tipos do domínio, validação estrutural do documento, catálogo versionado, entradas do operador, códigos de diagnóstico, tipos de snapshot e de falha; `builder` monta documentos por código (fixtures, exemplos, testes) |
| `circuit-sim` | `bitset`, `union_find`, `fingerprint`, `compiled`, `compile`, `solve`, `logic`, `runtime`, `trace` | Compilação, solver discreto, lógica de dispositivos, tick, estado, trace, checkpoint e replay; `bitset`, `union_find` e `fingerprint` são primitivas internas sem dependências |
| `circuit-project` | raiz do crate (`from_str`, `to_string`), `limits`, `migrate` | JSON canônico, leitura em duas fases, limites de tamanho, migrações |
| `apps/cli` | — | `validate`, `simulate`, `test` (e `export` em M3) |
| `xtask` | — | Verificação de arquitetura e atualização controlada de goldens |

### 5.3 Gatilhos para dividir em crates

Fronteira de módulo é barata de mudar; fronteira de crate não é. Um módulo vira crate quando **pelo menos um** dos gatilhos ocorrer:

1. **Segundo consumidor real** do mesmo tipo (por exemplo, o renderer e o exportador consumindo `Scene`).
2. **Dependência pesada ou não portável** que precisa ficar isolada (egui, clap, I/O).
3. **Alvo ou feature set divergente** (por exemplo, parte que não pode compilar para WASM).
4. **Contrato estável** por dois marcos sem alteração de assinatura pública.
5. **Tempo de compilação incremental** medido e incômodo.

Divisões previstas:

| Crate futuro | Origem | Gatilho esperado |
|---|---|---|
| `circuit-editor` (inclui comandos de edição, undo/redo, hit-test, roteamento) | novo | M2b: a GUI de edição começa |
| `circuit-renderer` (`Scene`, `build_scene`) | novo | M2a: viewer e, depois, exportador |
| `circuit-export` | novo | M3: SVG/PDF |
| `circuit-topology` (`compiled` + `compile`) | `circuit-sim` | Segundo consumidor do `CompiledCircuit` sem runtime (lint, análise estática, netlist) |
| `circuit-io` | novo | M4: adaptadores industriais |

### 5.4 Regras de dependência e verificação

```text
circuit-model      → serde, thiserror (allowlist em xtask)
circuit-sim        → circuit-model, thiserror
circuit-project    → circuit-model, serde_json, thiserror
apps/cli           → crates do núcleo + serde, serde_json (parser de argumentos próprio)
apps/desktop       → crates do núcleo + circuit-renderer + egui/eframe
xtask              → serde_json (lê `cargo metadata`), circuit-model, circuit-project (gera os exemplos)
```

Decisão do M1: o conjunto de bits do solver é um módulo próprio e pequeno (`bitset`) em vez de `fixedbitset`, e a CLI tem só os subcomandos `validate`, `simulate` e `test` com poucas opções, então usa um parser próprio em vez de `clap`/`anyhow`. Ambos mantêm a árvore de dependências mínima (§5.5). `clap` entra quando a CLI crescer o bastante para justificá-lo (gatilho 2 de §5.3), sem tocar nos crates do núcleo.

Regras obrigatórias:

1. `circuit-model` não depende de outros crates do workspace.
2. `circuit-sim` e `circuit-project` dependem apenas de `circuit-model` entre os crates do workspace.
3. Nenhum crate `circuit-*` depende, direta ou transitivamente, de egui, eframe, winit, clap ou anyhow.
4. Nenhum crate do núcleo usa relógio do sistema (`Instant`, `SystemTime`), `std::fs`, `std::net` ou threads.
5. Renderer e editor recebem `SimSnapshot` (tipo de `circuit-model`); nunca dependem de `circuit-sim`.
6. Dentro de `circuit-sim`:

```text
bitset, union_find, fingerprint, logic → (nada)
compiled   → fingerprint
compile    → compiled, fingerprint, union_find
solve      → bitset, compiled, union_find
trace      → compiled, fingerprint
runtime    → bitset, compiled, fingerprint, logic, solve, trace
```

Mecanismos de verificação:

- `cargo run -p xtask -- check-architecture`: usa `cargo metadata --locked` para checar as regras 1, 2, 3 e 5 (grafo de dependências resolvido, inclusive transitivas, contra allowlists que falham fechadas: um pacote ou módulo sem regra é violação) e examina os `use crate::…` de cada módulo de `circuit-sim` para a regra 6. A análise de módulos é textual (não resolve macros nem re-exports com glob); `pub(crate)` e revisão cobrem o resto.
- `clippy.toml` em cada crate do núcleo, com `disallowed-types` (`std::time::Instant`, `std::time::SystemTime`, `HashMap`, `HashSet`, `std::fs::File`, `std::net::TcpStream`, `std::net::UdpSocket`). O uso interno e ordenado de `HashMap` exige `#[allow]` com comentário (regra 4 e determinismo de saída). Não há `disallowed-methods`: o acesso a `std::env`, `std::fs`, `std::net`, `std::process` e `std::thread` no núcleo é recusado pela análise textual do `check-architecture` (`FORBIDDEN_CORE_APIS`).
- `cargo-deny` complementa com políticas de fontes, advisories e licenças; não substitui a validação das camadas internas.

### 5.5 Política de forks e dependências

Preferir crates do crates.io. Forks Git somente com:

- problema reproduzível que o upstream não resolve em release publicada;
- revisão de licença e segurança;
- commit imutável fixado;
- testes que demonstrem o benefício;
- responsável pela manutenção e estratégia de atualização;
- verificação periódica de que o motivo do fork ainda existe.

Se for adotado um fork de egui, toda a família relacionada (egui, eframe, egui-winit, winit) deve ficar em revisões compatíveis; não misturar tipos de upstream e fork na mesma aplicação. Qualquer `[patch.crates-io]` herdado de outro projeto deve ter o motivo documentado antes de ser copiado.

Dependências do núcleo são mínimas e justificadas uma a uma. Não há dependência de geração de UUID (§6.1) nem de biblioteca de grafos: o Union-Find necessário tem poucas dezenas de linhas.

### 5.6 Cargo.toml de referência

As versões abaixo são ilustrativas. Resolver com `cargo add`/`cargo update`, versionar `Cargo.lock` e executar o CI com `--locked`.

```toml
[workspace]
resolver = "3"
members = [                     # explícito, sem glob: um crate novo exige uma regra no xtask
    "apps/cli",
    "crates/circuit-model",
    "crates/circuit-project",
    "crates/circuit-sim",
    "xtask",
]                               # lista do M1; apps/desktop entrou no M2a e os demais crates, depois (§5.1)

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.85"           # a edição 2024 exige >= 1.85; a suíte passa nessa versão

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"

# testes (usados no M1)
proptest = { version = "1", default-features = false, features = ["std"] }

# candidatos de marcos futuros, ainda não usados: clap, anyhow (CLI maior),
# insta, tempfile, criterion (benchmarks)

# UI (M2a): não fixar no papel; usar `cargo add eframe` e deixar o Cargo.lock registrar.
```

**Nota sobre egui/eframe:** a versão não é fixada no documento. Em 13/11/2026 a versão mais recente observada no docs.rs era o eframe 0.36.1; a versão real é a resolvida por `cargo add` e registrada no `Cargo.lock`.

**Nota sobre MSRV:** não copiar o MSRV de outro projeto. Fixar `rust-toolchain.toml`, medir o MSRV do conjunto real de dependências e dos alvos suportados e só então preencher `rust-version`.

---

## 6. Modelo de domínio

### 6.1 Identidades

Um dispositivo representa o equipamento lógico ou físico. Um símbolo é uma representação gráfica desse dispositivo. Um terminal é um ponto de conexão estável de um símbolo. Essa separação evita que a bobina K1 e seus contatos sejam tratados como entidades sem relação.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub u64);    // idem: SymbolId, WireId, JunctionId, SupplyId

/// Maior ID aceito: seguro para consumidores JSON/JavaScript (2^53 - 1).
pub const MAX_ENTITY_ID: u64 = (1 << 53) - 1;

/// Contador único para todos os tipos de entidade; persistido como `next_id`.
pub struct IdAllocator { next: u64 }
impl IdAllocator {
    pub fn alloc(&mut self) -> u64 { let id = self.next; self.next += 1; id }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReferenceDesignator(pub String);   // "K1", "S2", "KT1"
```

Decisões **[decididas]**:

- IDs são `u64` alocados por um **contador único** do documento, a partir de 1 (0 é inválido), **nunca reutilizados**. IDs de tipos diferentes não colidem, o que simplifica `Subject` e diagnósticos. `uuid` sai da lista de dependências; colaboração em tempo real está fora do escopo e, se entrar, é uma migração de schema.
- Comandos de undo/redo guardam os IDs que alocaram, para que o redo recrie as mesmas entidades sem quebrar referências.
- **O vínculo entre símbolo e dispositivo é por `DeviceId`.** O designador (`K1`) é só um rótulo: renomear K1 para K7 não quebra os contatos. O editor resolve o texto digitado pelo usuário para um `DeviceId` existente (ou cria o dispositivo). Por isso o compilador não tem "referência de dispositivo não resolvida": referência inexistente é erro estrutural.
- Designador válido: `^[A-Z][A-Z0-9_]{0,15}$`, único **entre dispositivos**. Vários símbolos do mesmo dispositivo compartilham o designador por construção.

### 6.2 Referências a terminais

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TerminalKey(pub String);

pub struct PinRef {
    pub symbol_id: SymbolId,
    pub terminal: TerminalKey,
}
```

Os números exibidos (`13`, `14`) não são chaves: isso acoplaria o arquivo a uma convenção de numeração que o usuário pode querer mudar. Na prática:

- **A chave é estrutural** e pertence à definição do símbolo no catálogo: `a1`/`a2` (bobina), `p1`/`p2` (contato e botão), `com`/`no`/`nc` (reversor de contato), `pole1.in`/`pole1.out` … (contatos de força), `x1`/`x2` (lâmpada, buzzer), `u1`/`v1`/`w1` (motor), `out` (terminal de alimentação).
- **O rótulo exibido é derivado** pelo catálogo. Em contatos auxiliares, o rótulo vem de `(slot, tipo)`: NA no slot 1 exibe `13/14`; NC no slot 2 exibe `21/22`. O usuário pode sobrescrever o rótulo na instância (campo de apresentação) sem alterar chaves nem conectividade.
- O arquivo armazena chaves. O compilador as resolve para índices densos (`PinIx`).

### 6.3 Dispositivos e modelos

```rust
pub struct ModelRef { pub id: String, pub version: u32 }   // "contactor.generic", 1

pub struct Device {
    pub id: DeviceId,
    pub reference: ReferenceDesignator,
    pub model: ModelRef,
    pub params: BTreeMap<String, ParamValue>,   // validados pelo catálogo
}

pub enum ParamValue { Bool(bool), Int(i64), Text(String) }   // sem ponto flutuante; uma escolha de lista é um Text validado por ParamType::Enum

/// Propriedade do modelo no catálogo (não persistida no documento).
pub enum DeviceClass {
    Contactor, Relay, ThermalOverload, Timer,
    Pushbutton, LatchingSwitch, Fuse,
    Motor, Lamp, Buzzer, PowerSupply,
}
```

`DeviceClass` é uma enumeração **fechada no código** porque o comportamento de cada classe é implementado em `logic`. Já o `ModelRef` é aberto: vários modelos (`pushbutton_no`, `pushbutton_nc`) podem pertencer à mesma classe, e novos modelos não exigem mudança de schema. O arquivo persiste `ModelRef`, nunca a classe.

`LatchingSwitch` cobre seletora, disjuntor e botão de emergência com trava: um dispositivo com estado "fechado" que muda por ação explícita. `Fuse` é um elemento passivo; a fusão não é simulada.

### 6.4 Símbolos

```rust
/// Comportamento do símbolo no solver/lógica (propriedade do catálogo).
pub enum SymbolKind {
    Coil,
    ContactNo,
    ContactNc,
    Changeover,                       // com / no / nc
    PowerContacts { poles: u8 },
    ThermalElement { poles: u8 },
    FuseLink,
    Motor,
    Lamp,
    Buzzer,
    SupplyTerminal,
}

pub struct SymbolInstance {
    pub id: SymbolId,
    pub device_id: DeviceId,
    pub symbol_key: SymbolKey,        // chave no modelo: "coil", "aux_no", "aux_nc", "main"
    pub slot: Option<u8>,             // obrigatório para símbolos "slotted" (contatos auxiliares)
    pub position: GridPoint,
    pub rotation: QuarterTurn,
}
```

Botões de comando usam `ContactNo`/`ContactNc` com geometria própria; quem **comanda** cada contato depende da classe do dispositivo:

| Classe do dispositivo | Sinal que comanda os contatos do dispositivo |
|---|---|
| `Contactor`, `Relay` | `CoilActive` (efetivo após o commit, com atraso de atuação) |
| `Timer` | `TimerOut` (saída do temporizador) |
| `Pushbutton`, `LatchingSwitch` | `Actuated` (entrada externa; efetiva no mesmo tick) |
| `ThermalOverload` | `Tripped` (entrada externa com trava; efetiva no mesmo tick) |
| `Fuse` | sempre fechado |

Contato NA fecha quando o sinal está alto; NC fecha quando está baixo.

### 6.5 Posição e rotação

```rust
pub struct GridPoint { pub x: i32, pub y: i32 }

pub enum QuarterTurn { Deg0, Deg90, Deg180, Deg270 }
```

Coordenadas são unidades lógicas do documento, nunca pixels. A posição de cada pino é **derivada**: `posição do símbolo + offset do terminal (catálogo) rotacionado`.

### 6.6 Fios, junções e cruzamentos

```rust
pub struct Junction {
    pub id: JunctionId,
    pub position: GridPoint,
}

pub enum Endpoint {
    Terminal(PinRef),
    Junction(JunctionId),
}

pub struct Wire {
    pub id: WireId,
    pub from: Endpoint,
    pub to: Endpoint,
    pub route: Vec<GridPoint>,        // apenas pontos intermediários (waypoints)
}
```

- Os extremos do traçado são **derivados** dos endpoints (posição do pino ou da junção). Guardar só os waypoints evita que mover um símbolo invalide a geometria armazenada. `wire_polyline(&Schematic, &Catalog, WireId)` devolve a polilinha completa para renderização e hit-test.
- Roteamento ortogonal: segmentos consecutivos compartilham `x` ou `y`. O editor só gera traçados ortogonais; o carregamento aceita outros, e a compilação os reporta com o aviso `NonOrthogonalWire` (OC-STR-009), que nunca bloqueia a compilação.
- **Dois fios que apenas se cruzam não se conectam.** A conexão exige uma junção explícita. Um fio que passa sobre um terminal sem endpoint nele também não o conecta.

### 6.7 Condutores e alimentação

```rust
pub enum Conductor { L1, L2, L3, N, PE, DcPos, DcNeg }

/// Máscara de 1 byte; um bit por condutor (7 variantes).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ConductorSet(u8);
```

`Conductor` identifica a função do condutor. `PE` não é fonte de alimentação e não energiza cargas; `N` e `PE` não são intercambiáveis. A ligação N–PE depende do esquema de aterramento e do ponto de equipotencialização (tratada como aviso no MVP, §9.3).

```rust
pub struct SupplyProfile {
    pub id: SupplyId,
    pub kind: SupplyKind,
    /// Rótulo de tensão (mV): DC → tensão DC; monofásica → fase–neutro;
    /// trifásica → tensão de linha (fase–fase).
    pub nominal_voltage_mv: u32,
    /// Apenas trifásica com neutro: tensão fase–neutro.
    pub phase_voltage_mv: Option<u32>,
    pub frequency_millihz: Option<u32>,
    pub phase_sequence: Option<PhaseSequence>,
}

pub enum SupplyKind { Dc, SinglePhase, ThreePhase }
pub enum PhaseSequence { L1L2L3, L1L3L2 }
```

Uma fonte entra no circuito por um dispositivo da classe `PowerSupply`, com os parâmetros `conductor` e `supply` (um `SupplyId`), e um símbolo `SupplyTerminal` (terminal `out`). Cada ponto onde o condutor é "tirado" (L1 no topo do painel, 24 V em cada coluna) é uma instância de símbolo desse dispositivo.

Regras **[decididas]** para o MVP:

- **Seções de alimentação (M3, [ADR-0005](adr/0005-m3-circuit-semantics.md)):** cada `SupplyProfile` é uma seção galvanicamente separada, e um condutor presente num grupo é identificado por `(SupplyId, Conductor)`. Várias fontes podem fornecer o mesmo condutor; `ConflictingSupplies` (`OC-SEM-004`) deixou de ser emitido e o código fica reservado. Condutores vivos de seções diferentes no mesmo grupo são curto (`OC-ELEC-001`); `PE` é comum a todas as seções. No M1/M2 valia um perfil por condutor. O transformador de comando, que ligaria uma seção a outra, fica **[adiado]** (M4).
- Os campos de tensão e frequência são **rótulos**. Servem para verificar a compatibilidade de cada carga com a alimentação que a energiza (§9.4); não entram em nenhum cálculo de tensão ou corrente.

### 6.8 Documento

```rust
pub struct Schematic {
    pub schema_version: u32,
    pub next_id: u64,
    pub project: ProjectMeta,           // name, grid_unit (dica de editor; sem efeito semântico)
    pub supplies: Vec<SupplyProfile>,
    pub devices: Vec<Device>,
    pub symbols: Vec<SymbolInstance>,
    pub junctions: Vec<Junction>,
    pub wires: Vec<Wire>,
}
```

Invariantes verificados na compilação (fase estrutural): IDs únicos entre todas as coleções; todo ID `<= MAX_ENTITY_ID` e `< next_id`; toda referência aponta para entidade existente. Na serialização, cada coleção é ordenada por ID.

---

## 7. Catálogo de componentes

O catálogo é a fonte de verdade para modelos, terminais, rótulos, parâmetros, limites de instâncias, geometria e proveniência dos símbolos.

```rust
pub struct ComponentModel {
    pub id: String,                       // "contactor.generic"
    pub version: u32,                     // imutável após publicado
    pub class: DeviceClass,
    pub params: Vec<ParamDefinition>,
    pub symbols: Vec<SymbolDefinition>,
}

pub struct SymbolDefinition {
    pub key: SymbolKey,                   // "coil", "aux_no", "aux_nc", "main"
    pub kind: SymbolKind,
    pub terminals: Vec<TerminalDefinition>,
    pub min_instances: u8,
    pub max_instances: u8,                // ex.: aux_no: 0..=8
    pub slotted: bool,                    // exige `slot` único por dispositivo
    pub geometry: SymbolGeometry,
    pub standard_reference: Option<String>,
    pub provenance: Provenance,           // origem, revisão, licença
}

pub struct TerminalDefinition {
    pub key: TerminalKey,                 // estrutural (§6.2)
    pub label: LabelRule,                 // rótulo derivado
    pub required: bool,                   // exige >= 1 conexão (§13.3)
    pub offset: GridPoint,
}

pub struct ParamDefinition { pub key: String, pub ty: ParamType, pub default: ParamValue }
```

**Terminais pertencem a símbolos**, não ao dispositivo. Dois contatos NA do K1 são dois símbolos independentes, cada um com seus terminais. O dispositivo guarda apenas `ModelRef` e parâmetros.

**Versionamento:** um modelo publicado é imutável; mudar chave de terminal, semântica ou parâmetros cria uma nova `version`. O arquivo guarda `(id, version)`. Ao carregar, uma versão desconhecida gera `UnknownModel`; uma versão antiga com regra de compatibilidade registrada é migrada (`ModelMigration`) e a migração é testada.

**Origem do catálogo no MVP:** definido em código Rust (tipado e testado). No M4, bibliotecas de terceiros só de dados estendem o catálogo, reusando as classes de comportamento do núcleo ([ADR-0008](adr/0008-extension-libraries.md)).

Modelos iniciais:

| Modelo (`id`) | Classe | Símbolos (instâncias) | Parâmetros principais |
|---|---|---|---|
| `supply.terminal` | `PowerSupply` | `terminal` (1) | `conductor`, `supply` |
| `pushbutton_no` / `pushbutton_nc` | `Pushbutton` | `contact` (1) | — |
| `selector.latching` | `LatchingSwitch` | `contact` (1) | `initial_closed` |
| `main_switch.3p` | `LatchingSwitch` | `main` (1, três polos) | `initial_closed` |
| `contactor.generic` | `Contactor` | `coil` (1), `main` (0..1), `aux_no` (0..8), `aux_nc` (0..8) | `rated_voltage_mv`, `connection` |
| `relay.generic` | `Relay` | `coil` (1), `aux_no`, `aux_nc` (0..8); `changeover` (0..8) na versão 2 (M3, ADR-0005) | `rated_voltage_mv`, `connection` |
| `thermal_overload.generic` | `ThermalOverload` | `element` (0..1), `aux_nc` (95/96), `aux_no` (97/98) | — |
| `timer_on.generic` / `timer_off.generic` | `Timer` | `coil` (1), `delayed_no`, `delayed_nc` (0..4) | `preset_ms`, `rated_voltage_mv`, `connection` |
| `fuse.generic` | `Fuse` | `link` (1) | — |
| `motor.3ph` | `Motor` | `motor` (1) | `rated_voltage_mv` |
| `lamp.generic` / `buzzer.generic` | `Lamp` / `Buzzer` | 1 | `rated_voltage_mv`, `connection` |

`connection` ∈ {`dc`, `line_neutral`, `line_line`} e descreve como a carga se liga à alimentação (§9.4).

---

## 8. Compilação da topologia

### 8.1 Pipeline e contrato

```text
Schematic + ComponentCatalog
   │
   ├── 1. validar estrutura        IDs, referências, modelo/versão, símbolo/terminal, slots, limites
   ├── 2. tabela de pinos          (símbolo, terminal) → PinIx, em ordem canônica
   ├── 3. redes de cobre           Union-Find estático sobre pinos e junções via fios
   ├── 4. resolver dispositivos    designadores, sinais, perfis de alimentação
   ├── 5. montar elementos         switches, fontes, cargas
   ├── 6. verificações estáticas   terminais obrigatórios, redes flutuantes, séries, curtos permanentes
   └── 7. emitir                   CompiledCircuit + diagnostics
```

```rust
pub struct CompileOutput {
    /// `Some` se, e somente se, não houver diagnóstico de severidade Error.
    pub circuit: Option<CompiledCircuit>,
    /// Todos os diagnósticos (inclusive avisos), em ordem canônica.
    pub diagnostics: Vec<Diagnostic>,
}

pub fn compile(schematic: &Schematic, catalog: &ComponentCatalog) -> CompileOutput;
```

`compile` devolve `CompileOutput`, e não `Result<_, Vec<Diagnostic>>`: avisos não somem em caso de sucesso, e o resultado parcial fica disponível mesmo em caso de erro. O editor exibe todos os diagnósticos sempre, e a CLI decide o código de saída.

Política de fases: a fase 1 roda por completo. Se produzir qualquer Error que invalide as tabelas das fases seguintes (ID duplicado, referência inexistente), a compilação para ali. Caso contrário, as fases 2–6 rodam e acumulam diagnósticos sem abortar na primeira falha.

### 8.2 Índices internos

```rust
pub struct PinIx(pub u32);
pub struct NetIx(pub u32);       // rede de cobre
pub struct SigIx(pub u32);
pub struct DeviceIx(pub u32);
pub struct SwitchIx(pub u32);
pub struct LoadIx(pub u32);
```

Newtypes impedem mistura acidental. São internos e densos; o documento persistido e o snapshot usam apenas IDs estáveis.

### 8.3 Representação compilada

```rust
pub struct CompiledCircuit {
    pub pins: Vec<CompiledPin>,          // PinIx → { pin: PinRef, net: NetIx }
    pub nets: Vec<CompiledNet>,          // redes de cobre estáticas
    pub switches: Vec<CompiledSwitch>,   // { a: NetIx, b: NetIx, drive: Drive, owner: SymbolId }
    pub sources: Vec<CompiledSource>,    // { net: NetIx, conductor: Conductor, supply: SupplyId, owner: DeviceId }
    pub loads: Vec<CompiledLoad>,        // { device: DeviceIx, symbol: SymbolId, rule: LoadRule, nets: Vec<NetIx> }
    pub signals: SignalTable,
    pub devices: Vec<CompiledDevice>,
    pub wires: Vec<(WireId, NetIx)>,         // para o snapshot colorir fios
    pub junctions: Vec<(JunctionId, NetIx)>,
    pub fingerprint: Fingerprint,
}

pub enum Drive {
    Always,                                   // fusível, elemento térmico
    Signal { sig: SigIx, closed_when: Level },
}
pub enum Level { Low, High }

pub struct SignalDef { pub owner: DeviceId, pub kind: SignalKind }
pub enum SignalKind {
    Actuated,     // botão pressionado / chave fechada (entrada externa)
    Tripped,      // térmico disparado (entrada externa com trava)
    CoilActive,   // contatos de contator/relé (derivado, via commit)
    TimerOut,     // saída de temporizador (derivado, via commit)
}
```

- Um **switch** liga duas redes de cobre (não pinos). Contato de força com `n` polos vira `n` switches com o mesmo `Drive`; `Changeover` vira dois switches com o mesmo sinal e `closed_when` opostos; `FuseLink` e `ThermalElement` viram switches `Always`.
- Cada sinal tem **um único escritor**: `Actuated` e `Tripped` por entradas externas (efetivos no mesmo tick); `CoilActive` e `TimerOut` pela lógica, no commit. Bobinas, botões e temporizadores nunca compartilham índices por acidente.
- `fingerprint` é um hash estável (FNV-1a de 64 bits sobre a codificação canônica na v1; trocável). Não usar `DefaultHasher`, cujo resultado não é estável entre versões.

### 8.4 Determinismo da compilação

1. Ordenar entidades por IDs estáveis antes de atribuir índices.
2. Numerar redes de cobre pelo menor `PinIx` do membro; nunca expor a raiz do Union-Find como identidade.
3. Canonicalizar listas de switches, fontes e cargas.
4. Ordenar diagnósticos por `(severidade desc., código, sujeito)`.
5. Nunca iterar `HashMap` em caminho que alimente saída.
6. Comparar resultados normalizados nos testes.

A garantia de determinismo bit a bit vale para todo o núcleo, porque ele só usa inteiros e booleanos. Um solver analógico futuro com ponto flutuante precisará de critérios numéricos próprios.

---

## 9. Solver elétrico discreto

### 9.1 Escopo

O solver trabalha com conectividade ideal e regras discretas. Determina quais redes pertencem ao mesmo grupo, quais condutores estão presentes em cada grupo, quais cargas estão energizadas e quais falhas ocorrem. Não calcula:

- tensão instantânea, corrente, impedância;
- queda ou divisão de tensão;
- transitórios;
- aquecimento físico, fusão de fusível, atuação de proteção.

### 9.2 Algoritmo por tick

```rust
pub struct Solver { /* buffers pré-alocados a partir do CompiledCircuit */ }

impl Solver {
    pub fn new(circuit: &CompiledCircuit) -> Self;
    pub fn solve(&mut self, closed: &SwitchMask) -> &ElectricalSolution;
}

pub struct ElectricalSolution {
    pub group_of_net: Vec<u32>,           // NetIx → grupo (canônico)
    pub group_conductors: Vec<ConductorSet>,
    pub loads: Vec<LoadState>,            // por LoadIx
    pub faults: Vec<ElectricalFault>,     // ordenadas
}

pub enum LoadState { Off, On, Rotating(Direction) }
```

O solver **não conhece sinais nem dispositivos**: recebe apenas o conjunto de switches fechados (`SwitchMask`, bitset por `SwitchIx`). O runtime converte sinais efetivos em `SwitchMask`. Assim `electrical` não depende do estado de simulação, e não há dependência cíclica entre camadas.

Passos:

1. Criar Union-Find sobre as **redes de cobre** (não sobre pinos).
2. Para cada switch fechado, unir `a` e `b`.
3. Para cada fonte, acrescentar o bit do condutor ao grupo da sua rede.
4. Avaliar cada grupo pelas regras de conflito (§9.3) e registrar falhas.
5. Avaliar cada carga pelas regras da §9.4, ignorando grupos em falha.
6. Detectar topologias não suportadas (§9.5).
7. Produzir a solução com falhas ordenadas.

Custo por tick: O((S + N) · α(N)), com S switches e N redes. Como a solução é função pura da máscara de switches, **se a máscara não mudou desde o tick anterior, a solução anterior é reutilizada** (otimização obrigatória, coberta por teste).

### 9.3 Curto-circuito e regras de conflito

A regra é aplicada à máscara de condutores de cada grupo:

| Máscara do grupo | Resultado |
|---|---|
| vazia ou um único condutor | normal |
| somente `{N, PE}` | aviso `NeutralEarthBond` (ponte N–PE é prática comum na origem da instalação) |
| `PE` com qualquer condutor ativo (`L1`, `L2`, `L3`, `DcPos`, `DcNeg`) | falha `ProtectiveEarthFault` |
| dois ou mais condutores ativos, incluindo `N` ou mistura AC/DC | falha `ShortCircuit` |

```rust
pub struct ShortCircuit {
    pub conductors: ConductorSet,
    pub pins: Vec<PinRef>,        // pinos do grupo, ordenados
    pub sources: Vec<DeviceId>,   // dispositivos-fonte envolvidos, ordenados
}
```

Não se infere que qualquer diferença de identificador equivale a curto em todos os contextos: a tabela é explícita e será estendida por perfil (TN/TT/IT) quando o aterramento for modelado **[adiado]**. Grupos em falha deixam de energizar as cargas ligadas a eles; o resto do circuito continua a ser avaliado.

Na compilação, a mesma tabela é aplicada às **redes de cobre** com todos os switches abertos: uma ligação permanente entre fontes incompatíveis é erro estático. Curtos que dependem de contatos fechados só aparecem em execução.

O simulador **não** simula atuação de disjuntor ou fusível: um curto é reportado como falha, não interrompido pelo modelo.

### 9.4 Cargas

Cada carga tem uma regra definida pelo modelo (`LoadRule`):

| Carga | Terminais | Energizada quando |
|---|---|---|
| Bobina, lâmpada, buzzer | dois | O par de grupos contém um par de condutores válido para `connection` **e** o rótulo de tensão é compatível |
| Motor (MVP) | `u1`, `v1`, `w1` | Os três grupos são distintos, cada um com exatamente uma fase, e juntos contêm L1, L2 e L3 |

Pares válidos para cargas de dois terminais:

- `dc`: `{DcPos, DcNeg}`; polaridade indiferente, salvo parâmetro `polarized = true`.
- `line_neutral`: `{L1|L2|L3, N}`.
- `line_line`: duas fases distintas.

**Compatibilidade de tensão (rótulos):** a tensão esperada vem do `SupplyProfile` que fornece os condutores do par: `dc` → `nominal_voltage_mv`; `line_neutral` → `phase_voltage_mv` em trifásica, `nominal_voltage_mv` em monofásica; `line_line` → `nominal_voltage_mv` da trifásica. A comparação é de igualdade inteira com `rated_voltage_mv`. Rótulo ausente, perfis diferentes entre os dois condutores ou valores distintos: a carga fica `Off` e o simulador emite `LoadSupplyMismatch` (aviso). Tolerância percentual fica **[a validar]**.

**Motor:** com as três fases presentes, o sentido é `Forward` se a ordem `(u1, v1, w1)` → fases acompanha o `phase_sequence` do perfil, e `Reverse` caso contrário. Duas fases presentes geram `PhaseLoss` e o motor fica `Off`. Isso **não** equivale a calcular torque, corrente ou velocidade; a interface mostra apenas parado, avante ou reverso. Motor de seis terminais (estrela-triângulo): **[decidido]** no M3 como uma única carga avaliada sobre os seis grupos, com padrões de estrela e de triângulo, enrolamentos abertos e ligação não suportada (`OC-MOD-004`); regra completa no [ADR-0005](adr/0005-m3-circuit-semantics.md).

### 9.5 Topologias não suportadas

O solver por conectividade ideal não resolve, de forma geral, cargas em série, pontos estrela flutuantes ou divisões de tensão. Não usar "potenciais virtuais" como atalho: isso poderia classificar redes inválidas como energizadas.

Estratégia:

- **MVP: rejeitar, não aproximar.**
  - *Estático:* rede de cobre formada **só** por terminais de carga, com terminais de duas ou mais cargas distintas e nenhum terminal de fonte ou switch, é série permanente → erro de compilação `StaticSeriesLoads`.
  - *Dinâmico:* grupo sem condutor que toque terminais de duas ou mais cargas distintas em execução → falha `DynamicUnsupportedTopology`. As cargas afetadas ficam `Off`; a política de falha (§10.6) decide se a simulação continua.
- **Etapa intermediária:** análise de caminhos e ramos, sem alegar cálculo analógico.
- **Etapa avançada:** solver resistivo (por exemplo, Modified Nodal Analysis) com modelos de componente e critérios de convergência.

A fronteira `solve(circuit, SwitchMask) → ElectricalSolution` permite substituir o solver sem mexer em lógica, runtime ou editor.

---

## 10. Motor lógico e semântica temporal

### 10.1 Tempo virtual e configuração

```rust
pub struct SimConfig {
    pub tick_us: u32,              // padrão: 1000 (1 ms) [a validar, §24]
    pub delays: DelayConfig,       // §10.5
    pub fault_policy: FaultPolicy, // §10.6
    pub record_trace: bool,
}
```

O **tick** é o relógio do motor: `t_us = tick * tick_us`. O runtime não lê o relógio do sistema. A GUI pode avançar zero, um ou vários ticks por frame (acumulando o tempo real e dividindo por `tick_us`), mas nunca altera a duração lógica do tick. Deve haver um teto de ticks por frame; ao atingi-lo, a GUI descarta o excesso e sinaliza "simulação atrasada" em vez de entrar em espiral.

### 10.2 Ordem de execução do tick

Seja `S` o conjunto de sinais efetivos no início do tick `n` e `T` o estado de temporizadores e contadores de atuação.

```text
1. entradas   S' = aplicar(S, ações de entrada do tick n)
                 Actuated/Tripped mudam NESTE tick.
2. contatos   closed = { sw | sw.drive == Always  ou  S'[sig] == sw.closed_when }
3. solver     sol = solve(circuit, closed)        (reutiliza a solução se `closed` não mudou)
4. lógica     (N, T') = evaluate(S', sol, T, config)
                 N = sinais derivados (CoilActive, TimerOut) para o PRÓXIMO tick.
5. registro   trace ← mudanças de S' em relação ao tick anterior,
                       transições de energização de cargas, início/fim de falhas
6. snapshot   disponível sob demanda: (n, S', sol)
7. commit     S ← S' com os sinais derivados substituídos por N;  T ← T';  n ← n + 1
```

**Contrato do snapshot [decidido]:** o snapshot do tick `n` representa o estado **efetivamente resolvido** naquele tick (`S'` e `sol`), antes do commit. Os estados seguintes não aparecem nele; são publicados como efetivos no snapshot do tick `n + 1`. Assim a imagem de fios, contatos e cargas é sempre fisicamente coerente consigo mesma.

Consequência estrutural: um contato comandado por bobina muda **pelo menos um tick depois** da energização da bobina, e um contato comandado por botão muda **no mesmo tick** da ação.

### 10.3 Entradas

```rust
pub enum InputAction {
    Press(DeviceId),            // Pushbutton
    Release(DeviceId),
    SetClosed(DeviceId, bool),  // LatchingSwitch
    Trip(DeviceId),             // ThermalOverload: dispara a trava
    ResetTrip(DeviceId),
}

/// Unidade de cenário e de log de replay.
pub struct InputEvent {
    pub tick: u64,
    pub action: InputAction,
}
```

Regras:

- Entradas são identificadas por `DeviceId`. Cenários e CLI usam o designador (`S1`) e o resolvem para `DeviceId` na carga.
- `step` valida **todas** as ações antes de aplicar qualquer uma. Alvo inexistente, classe incompatível (`Press` em contator) ou duas ações no mesmo dispositivo no mesmo tick retornam `SimulationError::InvalidInput`, **sem alterar o estado nem consumir o tick**.
- Ações são idempotentes: `Press` em botão já pressionado não gera evento de trace.
- `Trip` liga a trava do térmico; só `ResetTrip` a limpa. Rearme automático e curva de disparo ficam **[adiados]**.
- A GUI registra o tick lógico em que cada ação do usuário foi aplicada. Esse log (`Vec<InputEvent>`) é a fonte do replay (§17.4); eventos de ponteiro e de frame nunca entram no motor.

### 10.4 Estado e interface do simulador

```rust
pub struct RuntimeState {
    pub tick: u64,
    pub signals: SignalBits,            // efetivos no início do tick
    pub timers: Vec<TimerState>,        // um por dispositivo Timer
    pub actuators: Vec<ActuatorState>,  // contadores de pickup/dropout, um por Contactor/Relay
    pub faulted: bool,
}
```

O `next` é calculado a cada tick por função pura de `(S', sol, T)` e vive apenas como temporário; o estado observável e o checkpoint ficam menores.

```rust
pub struct Simulator { /* config, circuit, solver, state, trace */ }

#[derive(Clone, Copy)]
pub struct StepOutcome { pub tick: u64, pub changed: bool, pub faulted: bool }

impl Simulator {
    pub fn new(circuit: CompiledCircuit, config: SimConfig) -> Result<Self, ConfigError>;

    pub fn step(&mut self, inputs: &[InputAction])
        -> Result<StepOutcome, SimulationError>;

    /// Avança `n` ticks; as entradas valem apenas para o primeiro.
    pub fn step_n(&mut self, n: u32, first_tick_inputs: &[InputAction])
        -> Result<StepOutcome, SimulationError>;

    /// Estado resolvido do último tick; `None` antes do primeiro `step`.
    pub fn snapshot(&self) -> Option<SimSnapshot>;

    pub fn take_trace(&mut self) -> Vec<TraceEvent>;
    pub fn checkpoint(&self) -> Checkpoint;
    pub fn restore(&mut self, cp: &Checkpoint) -> Result<(), RestoreError>;
    pub fn reset(&mut self);
    pub fn reset_fault(&mut self);
    pub fn diagnostics(&self) -> &[Diagnostic];   // avisos de configuração (ex.: PresetQuantized)
}
```

- `step` **não aloca snapshot**: devolve um `StepOutcome` de poucos bytes. A GUI chama `snapshot()` uma vez por frame, não uma vez por tick (a 1 ms e 60 fps são ~16 ticks por frame).
- `ConfigError` ocorre para `tick_us == 0` ou atrasos de atuação nulos (`InvalidConfig`). Presets de temporizador que não são múltiplos do tick são arredondados para cima e geram `PresetQuantized`.
- Para reprodução devem ser registrados: `fingerprint` do circuito compilado, `SimConfig`, estado inicial e o log ordenado de `InputEvent`.

### 10.5 Atraso de atuação

```rust
pub struct DelayConfig {
    pub pickup_ticks: u32,    // padrão 1; mínimo 1
    pub dropout_ticks: u32,   // padrão 1; mínimo 1
}
```

O mínimo de **1 tick é estrutural**, não um valor "realista": com atraso zero, uma bobina que fecha o próprio contato de selo (ou um contato NF que realimenta a própria bobina) criaria um laço combinacional dentro do mesmo tick, sem solução definida.

Semântica (por bobina de contator ou relé, com contadores em ticks):

- Seja `run` o número de ticks consecutivos em que a bobina foi resolvida como energizada. Quando `run >= pickup_ticks`, o sinal `CoilActive` passa a alto no próximo commit; se a energização cai antes disso, o contador zera e o contator não "puxa".
- Simetricamente, com a bobina desenergizada por `dropout_ticks` ticks consecutivos, `CoilActive` passa a baixo.

Com os padrões (1, 1), o contato muda efetivo no tick seguinte ao da energização, como na §10.2. O atraso padrão não representa o tempo mecânico de um contator real; valores por dispositivo (`pickup_ms`/`dropout_ms` no modelo) ficam **[adiados]**.

A oscilação de um circuito com contato NF realimentando a própria bobina é determinada por estados e atrasos, **nunca pelo frame rate**: com (1, 1) o contator alterna a cada tick (período de 2 ticks) enquanto o comando persistir, e esse comportamento é testado (§17.2).

### 10.6 Falhas e política pós-falha

```rust
pub enum FaultPolicy {
    Halt,     // padrão (fail-closed): diagnósticos que travam interrompem a simulação
    Report,   // registra e continua; uso didático e de diagnóstico
}
```

- Só diagnósticos com `latches_runtime()` verdadeiro travam: `OC-ELEC-001` (`ShortCircuit`, inclui mistura AC/DC) e `OC-ELEC-002` (`ProtectiveEarthFault`). Os demais nunca interrompem, em nenhuma política (§13.2).
- Em ambas as políticas, um grupo com falha deixa de energizar suas cargas; o restante do circuito continua sendo avaliado. Falhas aparecem no snapshot enquanto estiverem ativas e geram eventos de início e fim no trace (por transição, não por tick).
- `Halt`: o tick em que surge uma falha que trava é concluído normalmente (snapshot com `faulted = true`, trace com a falha). A partir daí o runtime fica travado: `step` e `step_n` retornam `SimulationError::AlreadyFaulted` sem alterar o estado.
- `reset_fault()` limpa somente a trava. Não reinicia o circuito nem as entradas; se a condição que causou a falha persistir, ela é detectada de novo no tick em que se repetir. `reset()` volta ao tick 0. `restore()` volta a um checkpoint, que também carrega o estado de trava (§17.4).
- `Report`: não há trava. O reversor com acionamento simultâneo oscila, como descrito em §17.2.

A CLI expõe `--fault-policy halt|report` (padrão `halt`). A GUI também usa `Halt` por padrão.

---

## 11. Temporizadores e dispositivos

Seja `P >= 1` o preset em ticks: `P = max(1, ceil(preset_ms * 1000 / tick_us))`. A entrada do temporizador é o estado da carga `coil` do dispositivo na solução do tick (`entrada = (sol.loads[coil] != Off)`).

**Regra de fronteira [decidida]:** a entrada observada no tick `n` é avaliada **antes** de o contador do tick `n` ser atualizado. Se a entrada muda no mesmo tick em que o contador alcançaria `P`, prevalece a nova entrada.

### 11.1 TON (retardo na energização)

```text
se entrada:  elapsed = min(elapsed + 1, P);   out = (elapsed >= P)
senão:       elapsed = 0;                      out = false
```

Com `P = 3` e entrada alta nos ticks 2 a 5:

```text
tick            0 1 2 3 4 5 6 7 8
entrada         0 0 1 1 1 1 0 0 0
elapsed (fim)   0 0 1 2 3 3 0 0 0
TimerOut (ef.)  0 0 0 0 0 1 1 0 0
```

O contato temporizado fecha `P` ticks depois do primeiro tick com entrada alta e abre um tick depois da queda da entrada.

### 11.2 TOF (retardo na desenergização)

```text
se entrada:        elapsed = 0;                          out = true
senão se out:      elapsed = min(elapsed + 1, P);        out = (elapsed < P)
senão:             elapsed = 0                           (out permanece false)
```

Com `P = 3` e entrada alta nos ticks 2 a 4:

```text
tick            0 1 2 3 4 5 6 7 8
entrada         0 0 1 1 1 0 0 0 0
elapsed (fim)   0 0 0 0 0 1 2 3 0
TimerOut (ef.)  0 0 0 1 1 1 1 1 0
```

A saída liga um tick depois da entrada e desliga `P` ticks depois do primeiro tick com entrada baixa. Uma nova entrada alta durante a contagem cancela a temporização (`elapsed = 0`, `out = true`).

### 11.3 Estado do temporizador

```rust
pub struct TimerState {
    pub elapsed_ticks: u32,
    pub out: bool,          // saída calculada no último tick; efetiva no tick corrente
}
```

Os dois algoritmos acima só precisam de `elapsed` e `out`; fases como `Idle`, `Timing`, `Active` e `Retiming` são derivadas, e a semântica observável (a tabela de ticks) é a especificação.

### 11.4 Matriz mínima de comportamento

| Dispositivo | Entrada/condição | Resultado |
|---|---|---|
| Botão NA | `Press` | Contato fecha no mesmo tick |
| Botão NF | `Press` | Contato abre no mesmo tick |
| Chave com retenção | `SetClosed(true)` | Contato NA fecha, NF abre; permanece até nova ação |
| Bobina (contator/relé) | Carga `On` por `pickup_ticks` | `CoilActive` alto; contatos associados atuam no commit |
| Bobina | Carga `Off` por `dropout_ticks` | `CoilActive` baixo |
| Relé térmico | `Trip` | `Tripped` alto no mesmo tick: NC (95/96) abre, NA (97/98) fecha; trava até `ResetTrip` |
| TON | Entrada alta por `P` ticks | Saída alta |
| TOF | Entrada baixa por `P` ticks após ter estado ligada | Saída baixa |
| Fusível | — | Sempre condutor (fusão não simulada) |
| Lâmpada / buzzer | Par de grupos válido e tensão compatível | Carga `On` |
| Motor | Três fases presentes | `Forward`/`Reverse` conforme a sequência; sem grandezas físicas |

Cada linha tem pelo menos um teste automatizado.

---

## 12. Snapshot, trace e integração com a interface

### 12.1 Snapshot

O snapshot é uma estrutura imutável com o estado resolvido de um tick (§10.2). **Usa apenas IDs de documento**, de modo que renderer e editor não dependam de índices internos de compilação.

```rust
pub struct SimSnapshot {
    pub tick: u64,
    pub t_us: u64,
    pub energized_devices: Vec<DeviceId>,        // cargas em estado On neste tick
    pub active_devices: Vec<DeviceId>,           // contatores/relés com CoilActive; temporizadores com saída alta
    pub actuated_devices: Vec<DeviceId>,         // botões pressionados e chaves com retenção fechadas
    pub tripped_devices: Vec<DeviceId>,          // relés térmicos disparados
    pub closed_symbols: Vec<SymbolId>,           // contatos comandados efetivamente fechados no solve
    pub live_wires: Vec<Live<WireId>>,           // { id, conductors: ConductorSet }; ausente = sem condutor
    pub live_junctions: Vec<Live<JunctionId>>,
    pub motors: Vec<MotorView>,                  // { device, Stopped | Forward | Reverse }
    pub timers: Vec<TimerView>,                  // { device, elapsed_ticks, preset_ticks, out }
    pub faulted: bool,
    pub faults: Vec<Diagnostic>,                 // falhas ativas neste tick
}
```

- Todos os vetores são ordenados por ID. Não usar `HashMap` na saída, para que a ordem de serialização e a comparação sejam estáveis.
- `energized_devices` (a bobina está sob alimentação) e `active_devices` (o contator está "armado") diferem durante o atraso de atuação; a interface pode mostrar os dois estados.
- O tipo vive em `circuit-model`. Editor e renderer o consomem sem depender de `circuit-sim`.

### 12.2 Trace de mudanças

O trace registra **mudanças**, não um snapshot por tick: 10 s a 1 ms seriam 10 mil snapshots, e o que importa em teste é a sequência de transições.

```rust
pub struct TraceEvent { pub tick: u64, pub kind: TraceKind }

pub enum TraceKind {
    Signal { device: DeviceId, kind: SignalKind, from: bool, to: bool },
    Load   { device: DeviceId, symbol: SymbolId, activity: LoadActivity },  // Off | On | Forward | Reverse
    FaultStarted { code: DiagnosticCode, subject: Subject, related: Vec<Subject> },
    FaultCleared { code: DiagnosticCode, subject: Subject, related: Vec<Subject> },
    Latched,      // o runtime travou ao fim deste tick (FaultPolicy::Halt)
    LatchReset,   // reset_fault() antes deste tick
}
```

Ordem dentro do mesmo tick (determinística): sinais de entrada, sinais derivados, cargas e falhas; dentro de cada grupo, por `DeviceId` e depois pelo restante da chave. O formato textual e o cabeçalho (fingerprint e configuração) são normativos em `SIMULATION.md`; exemplo na §23.

---

## 13. Diagnósticos

### 13.1 Categorias

| Categoria | Prefixo | Exemplos | Momento |
|---|---|---|---|
| Estrutural | `OC-STR-` | ID duplicado, terminal inexistente, junção sem fios | Carregamento e compilação |
| Semântica | `OC-SEM-` | Designador duplicado, terminal obrigatório ausente | Compilação |
| Elétrica | `OC-ELEC-` | Curto, falha de PE, carga incompatível | Compilação (casos estáticos) e execução |
| Limitação do modelo | `OC-MOD-` | Topologia fora do solver discreto | Compilação e execução |
| Configuração | `OC-CFG-` | Tick inválido, preset arredondado | Criação do simulador |

### 13.2 Estrutura e códigos

```rust
pub enum Severity { Info, Warning, Error }

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticCode {
    DuplicateId, DanglingReference, UnknownModel, /* … */
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str;           // "OC-STR-001"
    pub const fn default_severity(self) -> Severity;
    /// Verdadeiro só para falhas que travam o runtime sob `FaultPolicy::Halt` (§10.6).
    pub const fn latches_runtime(self) -> bool;
}

pub enum Subject {
    Document, Supply(SupplyId), Device(DeviceId), Symbol(SymbolId),
    Pin(PinRef), Wire(WireId), Junction(JunctionId),
}

pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub subject: Subject,
    pub related: Vec<Subject>,
    pub message: String,
}
```

Regras:

- O **código textual** (`OC-<CAT>-NNN`) é o identificador estável. O enum é `#[non_exhaustive]` e mapeado por `as_str()`; códigos nunca são renumerados nem reutilizados. Um teste que lista todos os códigos detecta alterações acidentais.
- Mensagens são derivadas do código e dos sujeitos e podem ser localizadas no futuro; nenhuma lógica depende do texto.
- Severidade `Error` na compilação impede a emissão do `CompiledCircuit`. Em execução, só diagnósticos com `latches_runtime()` obedecem à `FaultPolicy`; os demais nunca interrompem. `Warning` e `Info` nunca bloqueiam. A CLI oferece `--deny-warnings`.
- Numeração: dentro de cada categoria, de `001` na ordem da tabela. Os identificadores citados em `SIMULATION.md` (`OC-ELEC-004`, `OC-ELEC-005`, `OC-MOD-002`) são os desta tabela.

| Código | Nome | Sev. | Trava | Quando ocorre |
|---|---|---|---|---|
| OC-STR-001 | `DuplicateId` | Error | não | ID repetido em qualquer coleção |
| OC-STR-002 | `DanglingReference` | Error | não | Referência a dispositivo, símbolo ou junção inexistente |
| OC-STR-003 | `UnknownModel` | Error | não | Modelo ou versão ausente do catálogo, sem migração |
| OC-STR-004 | `UnknownSymbolKey` | Error | não | `symbol_key` não existe no modelo |
| OC-STR-005 | `UnknownTerminal` | Error | não | Chave de terminal não existe no símbolo |
| OC-STR-006 | `SymbolInstanceLimit` | Error | não | Violação de mínimo/máximo de instâncias ou `slot` ausente/duplicado |
| OC-STR-007 | `JunctionUnconnected` | Error | não | Junção sem nenhum fio |
| OC-STR-008 | `JunctionDangling` | Warning | não | Junção com apenas um fio |
| OC-STR-009 | `NonOrthogonalWire` | Warning | não | Segmento de fio não ortogonal |
| OC-STR-010 | `InvalidParameter` | Error | não | Parâmetro fora do tipo ou do domínio do catálogo |
| OC-STR-011 | `IdOutOfRange` | Error | não | ID zero, acima de `MAX_ENTITY_ID` ou `>= next_id` |
| OC-STR-012 | `InvalidSupplyProfile` | Error | não | Campos do perfil de alimentação incoerentes com o tipo |
| OC-STR-013 | `SelfLoopWire` | Error | não | Fio com os dois extremos no mesmo ponto |
| OC-STR-014 | `UnsupportedSchemaVersion` | Error | não | Documento em memória fora da versão de schema atual |
| OC-STR-015 | `DuplicateSheetNumber` | Error | não | Número de folha zero ou repetido (ADR-0011) |
| OC-STR-016 | `InvalidSheet` | Error | não | Moldura sem zonas, de tamanho inválido, fora do intervalo de coordenadas ou sobreposta a outra |
| OC-STR-017 | `WireCrossesSheet` | Warning | não | Fio que atravessa a borda de uma folha; a impressão o cortaria |
| OC-SEM-001 | `DuplicateReference` | Error | não | Designador repetido entre dispositivos distintos |
| OC-SEM-002 | `InvalidReference` | Error | não | Designador fora do padrão |
| OC-SEM-003 | `MissingRequiredTerminal` | Error | não | Terminal `required` sem conexão |
| OC-SEM-004 | `ConflictingSupplies` | Error | não | Mesmo condutor fornecido por mais de um `SupplyId`. **Retirado no M3** (seções, ADR-0005): não é mais emitido; o código fica reservado |
| OC-SEM-005 | `DeviceWithoutSymbols` | Warning | não | Dispositivo sem nenhum símbolo |
| OC-SEM-006 | `FloatingNet` | Warning | não | Rede com 2+ pontos inalcançável por qualquer fonte, mesmo com todos os switches fechados |
| OC-SEM-007 | `NoSupply` | Warning | não | Documento com cargas e nenhuma fonte |
| OC-SEM-008 | `UnusedSupply` | Info | não | Fonte sem carga alcançável |
| OC-SEM-009 | `InvalidInterlock` | Error | não | Intertravamento mecânico que não nomeia dois contatores distintos, ou contator em mais de um intertravamento (M3) |
| OC-SEM-010 | `InvalidTransformer` | Error | não | Secundário de transformador que não é monofásico, é nomeado por outro transformador ou também é derivado por um `supply.terminal` (M4) |
| OC-ELEC-001 | `ShortCircuit` | Error | sim | Condutores incompatíveis no mesmo grupo, estático ou em execução (inclui mistura AC/DC) |
| OC-ELEC-002 | `ProtectiveEarthFault` | Error | sim | PE em contato com condutor ativo |
| OC-ELEC-003 | `NeutralEarthBond` | Warning | não | N e PE no mesmo grupo |
| OC-ELEC-004 | `LoadSupplyMismatch` | Warning | não | Rótulo de tensão ou tipo de alimentação incompatível com a carga; carga fica `Off` |
| OC-ELEC-005 | `LoadTerminalsShorted` | Warning | não | Os dois terminais de uma carga estão no mesmo grupo; carga fica `Off` |
| OC-ELEC-006 | `PhaseLoss` | Warning | não | Motor com apenas parte das fases |
| OC-ELEC-007 | `InterlockContention` | Warning | não | Os dois contatores de um intertravamento mecânico tentaram fechar no mesmo commit; nenhum fechou (M3) |
| OC-MOD-001 | `StaticSeriesLoads` | Error | não | Cargas em série permanente (§9.5); erro de compilação |
| OC-MOD-002 | `DynamicUnsupportedTopology` | Error | não | Grupo flutuante tocando 2+ cargas em execução (§9.5); cargas afetadas ficam `Off` |
| OC-MOD-003 | `CircuitTooLarge` | Error | não | Documento maior do que o compilador consegue indexar |
| OC-MOD-004 | `UnsupportedWindingConnection` | Error | não | Enrolamentos do motor de seis terminais energizados em ligação que não é estrela nem triângulo (M3) |
| OC-CFG-001 | `InvalidConfig` | Error | não | `tick_us` ou atraso de atuação inválido |
| OC-CFG-002 | `PresetQuantized` | Warning | não | Preset de temporizador não é múltiplo do tick |

### 13.3 Terminais desconectados

Nem todo terminal desconectado é erro; a obrigatoriedade é definida no catálogo por terminal (`required`):

| Situação | Tratamento |
|---|---|
| Contato auxiliar não utilizado | Não é desenhado (`min_instances = 0`); sem diagnóstico |
| Terminal de contato sem fio | Permitido (`required = false`) |
| Bobina sem uma das conexões | Erro `MissingRequiredTerminal` (`a1`/`a2` são `required`) |
| Motor sem alimentação | Carga `Off`; sem erro. Alimentação parcial gera `PhaseLoss` em execução |
| Fonte sem carga | Informação `UnusedSupply` |
| Junção sem fios | Erro `JunctionUnconnected`; com um fio, aviso `JunctionDangling` |

---

## 14. Editor gráfico e renderização

### 14.1 UI

A aplicação desktop usa egui/eframe. Todo código de UI fica em `apps/desktop`; `circuit-editor` e `circuit-renderer` (quando existirem) não dependem de egui.

Regiões sugeridas:

- toolbar;
- biblioteca de componentes;
- canvas;
- inspetor de propriedades;
- painel inferior de diagnósticos e eventos (com os códigos `OC-<CAT>-NNN` e navegação até o sujeito);
- controles de simulação (executar, pausar, passo, acionamento de botões e chaves).

### 14.2 Coordenadas e câmera

O documento usa unidades lógicas inteiras (`GridPoint`). O núcleo e o formato não usam ponto flutuante; a camada de visualização pode usar `f32`:

```rust
pub struct Camera {
    pub offset: [f32; 2],
    pub zoom: f32,
}
```

Devem existir transformações direta e inversa (documento ↔ tela) para renderização, hit-test e edição, com testes de ida e volta.

### 14.3 Cena neutra

```rust
pub fn build_scene(
    schematic: &Schematic,
    catalog: &ComponentCatalog,        // geometria dos símbolos
    view: &View,
    snapshot: Option<&SimSnapshot>,
) -> Scene;
```

A `Scene` é uma lista de primitivas em **coordenadas lógicas**, cada uma com um estilo **semântico** (não uma cor) e, opcionalmente, o `Subject` que representa:

```rust
pub struct SceneItem {
    pub shape: Shape,                 // Line, Polyline, Rect, Circle, Text
    pub style: StyleKey,              // Wire { live: ConductorSet }, Symbol { active }, Selected, Label, …
    pub subject: Option<Subject>,     // para hit-test e seleção
}
```

O tema (`StyleKey` → cor e espessura) pertence à aplicação ou ao exportador. A cena é testável sem GUI e serve de entrada ao SVG (§18). Não depende de egui nem de `circuit-sim`.

### 14.4 Viewer read-only (M2a)

Antes do editor, o primeiro artefato visual é um viewer: abre um `.ocircuit`, compila, roda o `Simulator`, constrói a `Scene` a cada frame com `snapshot()` e permite acionar botões e chaves (`InputAction`). Ele exercita o contrato `SimSnapshot → Scene` e permite avaliar a qualidade dos símbolos e da legibilidade antes de investir no editor.

### 14.5 Desempenho

Criar benchmarks desde cedo com documentos de 100, 500 e 2.000 componentes, medindo: renderização, hit-test, roteamento, compilação, `step`/`step_n` e invalidação de cache. Usar culling por bounding box e invalidação seletiva. Não introduzir otimização complexa sem medição. Os orçamentos-alvo (por exemplo, quanto do frame de 16,6 ms a 60 fps o `step_n` pode consumir) partem do primeiro benchmark, registrado em [`benchmarks.md`](benchmarks.md) e regenerado com `cargo run -p xtask --release -- bench`; o custo de pintura do egui ainda não é medido.

Recompilação no editor: após cada transação concluída, imediatamente. O primeiro benchmark (`docs/benchmarks.md`: cerca de 4 ms para compilar 2.000 dispositivos) dispensa o debounce no M2b; compilação incremental fica **[adiada]** até que a medição a justifique. Editar descarta a simulação em curso, que pertence a um circuito que já não existe.

### 14.6 Undo e redo

Usar Command Pattern:

```rust
pub trait Command {
    fn apply(&mut self, doc: &mut Schematic) -> Result<(), CommandError>;
    fn revert(&mut self, doc: &mut Schematic) -> Result<(), CommandError>;
}
```

Regras:

- O histórico é **transacional**: uma operação composta que falha no meio reverte o que já aplicou; o documento nunca fica parcialmente alterado.
- Comandos que criam entidades guardam os IDs alocados (§6.1) e os reutilizam no redo.
- Movimentos contínuos (arrastar) são aglutinados em um único comando.
- Toda mutação do documento no editor passa por comandos. Uma transação é aplicada a uma cópia e só a substitui se todas as edições passarem, se o resultado respeitar os limites do formato (`circuit_model::limits`, os mesmos do loader) e se não surgir erro de integridade novo (`DuplicateId`, `IdOutOfRange`, `DanglingReference`, `SelfLoopWire`, `UnsupportedSchemaVersion`) em relação ao documento anterior. A verificação é feita a cada transação, em produção; não se usa `debug_assert`, que entraria em pânico.
- Arrastar mostra uma prévia (a transação aplicada a uma cópia, fora do histórico) e grava uma única transação ao soltar.

---

## 15. Persistência e formato `.ocircuit`

### 15.1 Formato e regras

JSON com extensão `.ocircuit`. Regras normativas (detalhadas em `FORMAT.md`):

- `schema_version` obrigatório (inteiro); `next_id` obrigatório.
- **Sem ponto flutuante** em nenhum campo: todos os valores numéricos são inteiros (`u32`, `i32`, `u64`, `i64`). Tensão em milivolts, frequência em milihertz, tempo em milissegundos.
- IDs são inteiros `<= MAX_ENTITY_ID` (2^53 − 1), seguros para consumidores JavaScript.
- Serialização **canônica**: campos em ordem fixa da struct; coleções ordenadas por ID; `BTreeMap` para parâmetros; indentação de 2 espaços; fim de linha LF; newline final. Salvar um arquivo canônico recém-carregado produz **os mesmos bytes**.
- `.gitattributes` fixa `eol=lf` para `*.ocircuit`, traces e fixtures, para que o CI em Windows não quebre comparações byte a byte.
- Validação antes de usar: o loader aplica `deny_unknown_fields` ao schema **atual** e os limites de tamanho (`FORMAT.md`). Ranges, unicidade e referências de ID **não** são checados pelo loader: são diagnósticos de `Schematic::validate` e do compilador (`OC-STR-001`, `OC-STR-002`, `OC-STR-011`), para que um projeto quebrado ainda abra e seja explicado.
- Campos desconhecidos **não são preservados**. Preservá-los exigiria `flatten` em toda struct e conflitaria com a validação estrita. Decisão: recusar o que não se entende (§15.3).
- A versão do modelo de componente (`model.version`) é independente do `schema_version` do documento.

### 15.2 Exemplo (comando da partida direta)

Trecho ilustrativo: as coordenadas dependem da geometria do catálogo, e campos opcionais `null` são abreviados. O formato normativo, com `null` explícito, está em `FORMAT.md`. O arquivo completo `examples/direct-starter` inclui o circuito de força e o motor.

```json
{
  "schema_version": 1,
  "next_id": 23,
  "project": { "name": "Partida direta - comando", "grid_unit": 10 },
  "supplies": [
    { "id": 1, "kind": "dc", "nominal_voltage_mv": 24000 }
  ],
  "devices": [
    { "id": 2, "reference": "P24", "model": { "id": "supply.terminal", "version": 1 }, "params": { "conductor": "dc_pos", "supply": 1 } },
    { "id": 3, "reference": "P0", "model": { "id": "supply.terminal", "version": 1 }, "params": { "conductor": "dc_neg", "supply": 1 } },
    { "id": 4, "reference": "S2", "model": { "id": "pushbutton_nc", "version": 1 }, "params": {} },
    { "id": 5, "reference": "S1", "model": { "id": "pushbutton_no", "version": 1 }, "params": {} },
    { "id": 6, "reference": "K1", "model": { "id": "contactor.generic", "version": 1 }, "params": { "rated_voltage_mv": 24000, "connection": "dc" } }
  ],
  "symbols": [
    { "id": 7,  "device_id": 2, "symbol_key": "terminal", "slot": null, "position": { "x": 0,  "y": 0 },  "rotation": "deg0" },
    { "id": 8,  "device_id": 3, "symbol_key": "terminal", "slot": null, "position": { "x": 0,  "y": 20 }, "rotation": "deg0" },
    { "id": 9,  "device_id": 4, "symbol_key": "contact",  "slot": null, "position": { "x": 0,  "y": 3 },  "rotation": "deg90" },
    { "id": 10, "device_id": 5, "symbol_key": "contact",  "slot": null, "position": { "x": -3, "y": 9 },  "rotation": "deg90" },
    { "id": 11, "device_id": 6, "symbol_key": "coil",     "slot": null, "position": { "x": 0,  "y": 16 }, "rotation": "deg90" },
    { "id": 12, "device_id": 6, "symbol_key": "aux_no",   "slot": 1,    "position": { "x": 3,  "y": 9 },  "rotation": "deg90" }
  ],
  "junctions": [
    { "id": 13, "position": { "x": 0, "y": 7 } },
    { "id": 14, "position": { "x": 0, "y": 13 } }
  ],
  "wires": [
    { "id": 15, "from": { "terminal": { "symbol_id": 7,  "terminal": "out" } }, "to": { "terminal": { "symbol_id": 9,  "terminal": "p1" } }, "route": [] },
    { "id": 16, "from": { "terminal": { "symbol_id": 9,  "terminal": "p2" } }, "to": { "junction": 13 }, "route": [] },
    { "id": 17, "from": { "junction": 13 }, "to": { "terminal": { "symbol_id": 10, "terminal": "p1" } }, "route": [{ "x": -3, "y": 7 }] },
    { "id": 18, "from": { "junction": 13 }, "to": { "terminal": { "symbol_id": 12, "terminal": "p1" } }, "route": [{ "x": 3,  "y": 7 }] },
    { "id": 19, "from": { "terminal": { "symbol_id": 10, "terminal": "p2" } }, "to": { "junction": 14 }, "route": [{ "x": -3, "y": 13 }] },
    { "id": 20, "from": { "terminal": { "symbol_id": 12, "terminal": "p2" } }, "to": { "junction": 14 }, "route": [{ "x": 3,  "y": 13 }] },
    { "id": 21, "from": { "junction": 14 }, "to": { "terminal": { "symbol_id": 11, "terminal": "a1" } }, "route": [] },
    { "id": 22, "from": { "terminal": { "symbol_id": 11, "terminal": "a2" } }, "to": { "terminal": { "symbol_id": 8, "terminal": "out" } }, "route": [] }
  ]
}
```

Lido como circuito: P24 → S2 (NF) → junção → [S1 (NA) em paralelo com o contato NA de K1] → junção → bobina K1 → P0.

### 15.3 Leitura em duas fases e política de versões

1. **Fase 1 (permissiva):** ler apenas `schema_version`, ignorando o restante.
   - `schema_version` **maior** que o suportado: erro `ProjectError::NewerSchema { found, supported }`, com mensagem clara. Não há tentativa de leitura parcial.
   - `schema_version` **menor**: aplicar as migrações em sequência sobre `serde_json::Value`.
2. **Fase 2 (estrita):** desserializar o `Value` já migrado para as structs do schema atual, com `deny_unknown_fields` e os limites de tamanho. A verificação de ranges de IDs fica na validação do documento (§8.1), não no loader.

Em seguida, a compilação (fase estrutural, §8.1) valida modelo, versão de modelo, símbolos e terminais contra o catálogo.

### 15.4 Responsabilidade de I/O

`circuit-project` converte entre texto JSON e estruturas, mas **não acessa `std::fs`**. A aplicação (desktop ou CLI) abre, grava e escolhe caminhos. O salvamento é atômico na camada de aplicação: escrever em arquivo temporário no mesmo diretório, sincronizar e renomear sobre o destino.

### 15.5 Migrações

- Cada migração `vN → vN+1` é uma função pura sobre `Value`, com um par de fixtures (antes/depois) e teste próprio.
- Toda mudança de schema incrementa `schema_version`, acrescenta migração e registra uma entrada no changelog de `FORMAT.md`.
- Não alterar silenciosamente a semântica de terminais ou componentes: mudança de modelo é nova `version` no catálogo (§7).

---

## 16. CLI headless

`apps/cli` executa e valida circuitos sem interface gráfica.

```text
opencircuit validate <projeto.ocircuit> [--format text|json] [--deny-warnings]
opencircuit simulate <projeto.ocircuit> --scenario <cenario.json>
                     [--tick-us N] [--fault-policy halt|report]
                     [--trace saida.trace] [--snapshot-json saida.json]
opencircuit test <caminho>... [--update-golden]
opencircuit export <projeto.ocircuit> --output <saida.svg> [--theme print|live]   # M3; formato pela extensão
```

Formato de cenário (referências por designador; a CLI resolve para `DeviceId`):

```json
{
  "schema_version": 1,
  "circuit": "command.ocircuit",
  "config": { "tick_us": 1000, "pickup_ticks": 1, "dropout_ticks": 1, "fault_policy": "report" },
  "events": [
    { "tick": 2,  "action": "press",   "device": "S1" },
    { "tick": 6,  "action": "release", "device": "S1" },
    { "tick": 10, "action": "press",   "device": "S2" },
    { "tick": 14, "action": "release", "device": "S2" }
  ],
  "until_tick": 20,
  "expect": { "trace": "command.trace", "diagnostics": [] }
}
```

`until_tick` é o último tick executado (inclusive). `expect.diagnostics` lista os códigos esperados na compilação.

Comportamento:

- Designador desconhecido ou ambíguo no cenário é erro de uso, com a lista de designadores existentes.
- `test` percorre arquivos `*.scenario.json`, compara o trace gerado com o golden e imprime diff legível. `--update-golden` reescreve os goldens e **é recusado quando a variável `CI` está definida**.
- Códigos de saída: `0` sucesso; `1` falha de validação (diagnóstico `Error`, ou `Warning` com `--deny-warnings`) ou de teste (trace divergente); `2` erro de uso ou de I/O.
- `--format json` produz saída estruturada para integração com CI.

---

## 17. Testes e qualidade

### 17.1 Testes unitários

- catálogo de componentes (terminais, rótulos derivados, limites de instâncias);
- resolução de terminais e fase estrutural do compilador;
- Union-Find estático e dinâmico, incluindo memoização por máscara de switches;
- contatos NA/NC, bobinas e atraso de atuação;
- TON e TOF, com as tabelas de ticks da §11 como casos de teste;
- térmico, chave com retenção e fusível;
- cargas: pares válidos, rótulos de tensão, motor (sentido e perda de fase);
- regras de conflito de condutores (§9.3) e topologias não suportadas (§9.5);
- serialização canônica, rejeição de schema mais novo, migrações;
- transformações de coordenadas e hit-test (a partir do M2a/M2b);
- comandos de undo/redo (M2b).

### 17.2 Golden tests

Cada exemplo contém: documento `.ocircuit`, cenário com eventos de entrada e configuração temporal, trace esperado e diagnósticos esperados. O trace é o **log de mudanças** da §12.2, em texto, comparado byte a byte.

**Partida direta (comando)** — S1 NA, S2 NF, K1 com contato de selo. Trace de referência na §23. Na versão completa, com motor como carga simplificada e relé térmico (`Trip` abre o NC 95/96 e desliga K1).

**Reversor** — K1 e K2, intertravamento elétrico por contatos NF, sequência de fases invertida no circuito de força. Três casos:

1. *Sentido normal:* S1 liga K1; K2 permanece inativo; S2 desliga tudo.
2. *Inversão bloqueada:* com K1 ativo, S3 não energiza K2 (o NF de K1 está aberto); em nenhum tick há `CoilActive` simultâneo.
3. *Acionamento simultâneo:* S1 e S3 pressionados no mesmo tick. Com atrasos (1, 1), as duas bobinas energizam no tick `n` (ambos os NF ainda fechados); no tick `n + 1` os dois contatores estão ativos, as duas bobinas caem e o circuito de força registra `ShortCircuit` (`OC-ELEC-001`); no tick `n + 2` ambos estão inativos e o ciclo recomeça, com **período de 2 ticks enquanto os botões estiverem pressionados**. Com a política padrão `halt` (fail-closed), o runtime trava em `n + 1`: o tick é concluído com `faulted = true` e os passos seguintes retornam `AlreadyFaulted` até `reset_fault()`. A oscilação descrita acima ocorre com `--fault-policy report`. Esse é o comportamento do modelo ideal síncrono sem intertravamento mecânico; a trava é o resultado esperado do golden, não um defeito a esconder.

**Estrela-triângulo** — KM1 principal, KM2 triângulo, KM3 estrela, KT1 temporizador com contato comutador; intertravamento elétrico (NF auxiliares) e mecânico entre KM2 e KM3. Implementado no M3 (`examples/star-delta/`) com o modelo de enrolamentos do ADR-0005: goldens de partida (estrela, intervalo com enrolamentos abertos, triângulo), parada em estrela e em triângulo, disparo térmico e atraso de queda maior que o de fechamento; um teste de propriedades (`xtask`) mostra que, para quaisquer ações do operador, preset e atrasos, KM2 e KM3 nunca estão ativos no mesmo tick e o circuito nunca entra em falha. A fixture `tests/fixtures/star-delta-overlap` (sem intertravamentos, queda de 3 ticks) mostra a sobreposição detectada como curto `OC-ELEC-001` no ponto estrela.

### 17.3 Testes de propriedades

Com `proptest`:

- permutar a ordem de dispositivos, símbolos, fios e junções não altera os resultados normalizados da compilação;
- recompilar o mesmo documento gera a mesma representação canônica e o mesmo `fingerprint`;
- `load → save` de arquivo canônico reproduz os mesmos bytes;
- salvar e carregar preserva a semântica (compilar antes e depois dá o mesmo resultado);
- `step_n(n)` equivale a `n` chamadas de `step` com as mesmas entradas;
- clone/checkpoint/restore e replay reproduzem o trace;
- operações de conexão e desconexão (M2b) não deixam referências inválidas.

### 17.4 Replay e rewind

`Clone` copia estado, mas não implementa rewind. A estratégia combina:

```text
Input Event Log ─► Checkpoints periódicos ─► Restauração ─► Replay determinístico
```

Um `Checkpoint` contém o `fingerprint` do circuito, o hash da `SimConfig`, o tick, os sinais efetivos, os temporizadores, os contadores de atuação, o estado de `faulted` e o conjunto de falhas ativas (para que as transições do trace continuem corretas). `restore` recusa um checkpoint de circuito ou configuração diferentes (`RestoreError`). Para voltar ao tick `t`, restaura-se o checkpoint mais próximo anterior e reproduz-se o log de entradas. O intervalo entre checkpoints é configurável e definido por medição **[a validar]**.

### 17.5 UI

Usar `egui_kittest` para fluxos essenciais, verificando antes sua disponibilidade para a versão do egui adotada: abrir exemplo; selecionar componente; conectar terminais; acionar botão; observar o estado do contator; salvar e reabrir.

### 17.6 CI

Matriz inicial: Linux, Windows e macOS.

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo test --workspace --locked
cargo deny check
cargo run --locked -p xtask -- check-architecture
cargo check --locked --target wasm32-unknown-unknown \
      -p circuit-model -p circuit-sim -p circuit-project -p circuit-renderer \
      -p circuit-commands -p circuit-editor -p circuit-export -p circuit-io
cargo clippy --locked --target wasm32-unknown-unknown -p opencircuit-desktop \
      -- -D warnings                                                    # aplicação web
cargo build --locked --release --target wasm32-unknown-unknown -p opencircuit-desktop
cargo +1.85 test --workspace --exclude opencircuit-desktop --locked   # MSRV do núcleo
```

O CI nunca executa `--update-golden`. Desde o M4 o CI também passa o clippy e compila a aplicação web (`opencircuit-desktop` para `wasm32-unknown-unknown`), porque compilar o núcleo para WASM não garante a portabilidade da aplicação inteira. O build não é publicado.

---

## 18. Exportação e símbolos

### 18.1 Exportação

O exportador recebe `Scene` (coordenadas lógicas e estilos semânticos), não estruturas de egui. Formatos planejados:

- **SVG:** escrito diretamente a partir da `Scene`; é texto e dispensa biblioteca.
- **PDF:** **[decidido]** no M3 após spike: `pdf-writer` (MIT OR Apache-2.0, poucas dependências, compila no MSRV 1.85 e em WASM), uma página do tamanho do desenho, bytes determinísticos ([ADR-0006](adr/0006-pdf-export.md)) e texto em um subconjunto embutido da Liberation Mono, que mantém acentos, grego e cirílico ([ADR-0012](adr/0012-pdf-font.md)).
- PNG ou outros formatos vetoriais, se necessário.

A exportação é executável pela CLI.

### 18.2 Símbolos e normas

O projeto cria símbolos próprios seguindo convenções eletrotécnicas documentadas. Não copiar arquivos gráficos de bibliotecas externas sem verificar licença e autorização.

A IEC 60617 é referência importante para símbolos gráficos eletrotécnicos, e devem ser consideradas as normas nacionais aplicáveis às jurisdições atendidas (por exemplo, ABNT). O uso de símbolos não deve ser descrito como conformidade normativa certificada sem revisão técnica e jurídica apropriada.

Cada `SymbolDefinition` carrega metadados de origem, revisão, licença e referência normativa (`Provenance`, `standard_reference`), sem incorporar conteúdo protegido sem licença. Os arquivos em `assets/symbols/` registram a proveniência por arquivo.

---

## 19. Segurança, licenças e governança

### 19.1 Licença

Licença pretendida: dual MIT OR Apache-2.0. Manter os arquivos de licença na raiz e incluir política para contribuições e dependências. Não incorporar código GPL ou outras licenças incompatíveis sem decisão explícita sobre a licença e a distribuição do projeto; a compatibilidade é analisada dependência por dependência (`cargo-deny`).

### 19.2 Arquivos não confiáveis

Um `.ocircuit` pode vir de terceiros. O loader deve:

- impor limites configuráveis de tamanho do arquivo, profundidade e contagem de entidades (valores iniciais definidos com os benchmarks da §14.5);
- rejeitar IDs fora do range, schema mais novo e campos desconhecidos (§15);
- nunca executar nada a partir do arquivo (não há scripts nem plugins no MVP);
- resistir a entrada arbitrária. No M2b isso é verificado por proptests no CI estável: texto e JSON arbitrários (com as chaves do schema), arquivo canônico corrompido e aninhamento profundo no loader; documentos arbitrários (IDs repetidos ou zero, modelos e chaves desconhecidos, parâmetros extremos, extremidades pendentes, coordenadas extremas) compilados e simulados; transações arbitrárias, que se aplicam por inteiro ou não se aplicam. Fuzzing com `cargo-fuzz` fica **[adiado]**: exige nightly, cria um `Cargo.lock` aninhado e o `libfuzzer-sys` usa a licença NCSA, fora do allowlist.

### 19.3 Governança

Arquivos recomendados:

```text
.github/
├── ISSUE_TEMPLATE/
├── workflows/
│   ├── ci.yml
│   └── release.yml
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
└── SECURITY.md
```

Políticas:

- toda funcionalidade do motor tem testes;
- todo modelo de componente declara seus terminais, rótulos e limites;
- mudanças de schema exigem migração e entrada em `FORMAT.md`;
- mudanças de comportamento do tick, dos temporizadores ou do solver exigem atualização de `SIMULATION.md` e de golden;
- dependências Git exigem revisão e responsável;
- recursos gráficos têm proveniência e licença registradas.

---

## 20. Roadmap

A numeração de marcos segue [`ROADMAP.md`](../ROADMAP.md), que é a fonte operacional. A ordem de trabalho é: contratos (M0), fatia headless (M1), viewer (M2a), editor (M2b). O viewer não começa antes da saída do M1.

### M0 — Fundação e contratos (concluído)

- Workspace, toolchain, resolver 3 e membros explícitos.
- IDs `u64` por tipo; separação dispositivo, símbolo e terminal; chaves estruturais de terminal.
- Condutores e perfis de alimentação; diagnósticos e validação estrutural iniciais.
- `xtask check-architecture` e CI com fmt, clippy, testes, `cargo-deny` e arquitetura.

**Critério:** contratos documentados que compilam e têm testes iniciais.

### M1 — Fatia vertical headless (concluído)

- Consolidar `circuit-domain` e `circuit-diagnostics` em `circuit-model`.
- `circuit-sim` com módulos de topologia, elétrica, lógica, runtime e trace.
- Redes de cobre estáticas; união por tick apenas pelos switches fechados.
- Solver elétrico discreto com máscara de condutores e regras de curto por perfil.
- Catálogo para Q1, F1, S1, S2, K1 e motor trifásico simplificado.
- Botões, chave com retenção, bobinas, contatos, TON/TOF e relé térmico com trava.
- Golden da partida direta e golden do reversor, incluindo o acionamento simultâneo (§17.2, caso 3).
- `circuit-project` JSON v1 com `next_id`, validação e round-trip.
- `apps/cli` com `validate`, `simulate` e `test`.
- Trace de mudanças, não um snapshot por tick.

**Critério:** cenários de comando e de força rodam sem GUI com traces determinísticos; o caso simultâneo do reversor trava no primeiro curto; round-trip JSON e códigos de saída da CLI testados; propriedades da §17.3 verdes.

### M2a — Viewer read-only (concluído)

- `circuit-renderer` com cena neutra (§14.3).
- `apps/desktop` com egui/eframe, em modo leitura.
- Abrir `.ocircuit`, compilar e simular.
- Play, pause, passo, reset e velocidade.
- Cores por condutor, contatos fechados, bobinas energizadas, motor e estado térmico.
- Painel de diagnósticos de compilação e de execução.
- Um `step_n` e um `snapshot()` por frame (§12, §14.5).
- Benchmarks de renderização com 100, 500 e 2.000 componentes.

**Critério:** abrir a partida direta, reproduzir, pausar e inspecionar diagnósticos; `circuit-renderer` não depende de `circuit-sim`.

### M2b — Editor desktop (concluído)

- `circuit-editor` e `circuit-commands`: comandos transacionais, undo/redo, hit-test e roteamento ortogonal (§14.6).
- Canvas e câmera, biblioteca de componentes, seleção, movimentação, rotação, exclusão, fios e junções, inspetor.
- Salvamento atômico (§15.4) e migrações iniciais do JSON.
- `egui_kittest` nos fluxos essenciais; testes de propriedade do loader (§19.2; o `cargo-fuzz` ficou adiado).

**Critério:** desenhar, salvar, reabrir e simular uma partida direta, com undo/redo sem aplicação parcial.

### M3 — Circuitos avançados e exportação (concluído)

- Contatos de comutação e intertravamento completo.
- Motor de seis terminais e enrolamentos.
- Estrela-triângulo sem sobreposição de contatores.
- Múltiplas seções de alimentação galvanicamente separadas.
- Exportação SVG a partir da cena neutra (`circuit-export`); PDF após spike (§18.1).
- Benchmarks para documentos maiores.

**Critério:** estrela-triângulo validada com modelo de enrolamentos adequado e sem sobreposição indevida; SVG determinístico; PDF escolhido com justificativa registrada ou explicitamente adiado.

### M4 — Ecossistema (concluído)

- Aplicação WASM, compilada e verificada no CI e não publicada.
- Extensões só de dados, sem plugins de código (ADR-0008).
- Modbus TCP por adaptador (`circuit-io`), servido por `opencircuit serve`; OPC UA e MQTT avaliados e não implementados (ADR-0010).
- Catálogo orientado a dados e símbolos de terceiros; mais componentes e exemplos.
- Documentação de extensão e comunidade.

**Critério:** plataforma extensível, com interfaces de integração documentadas e limites de segurança explícitos.

### M5 — Precisão do editor, folhas elétricas e automação pela CLI

- Snap com tolerância a pinos, junções e fios, com marcadores; grade visível; teclado para ferramentas, deslocamento, seleção total e zoom.
- Duplicar, copiar e colar com renumeração de designadores; dobras de fio preservadas; edição de vários elementos; alinhar e distribuir.
- Rótulos de terminal (A1/A2, 13/14) no catálogo, nas bibliotecas e no desenho.
- Folhas como molduras sobre a tela única, com tamanho de papel, zonas e carimbo (ADR-0011, schema v2); uma página de PDF por folha.
- Referências cruzadas entre bobina e contatos (folha e coluna), geradas e nunca persistidas.
- Saída JSON em todos os comandos; `opencircuit new` e `opencircuit edit --apply` com transações em JSON (só dados, sem motor de script; MCP fora do M5).

**Critério:** um projeto feito só pela CLI, com duas folhas, rótulos de terminal e referências cruzadas, valida, simula e exporta como PDF de várias páginas; no editor, snap, duplicação e deslocamento funcionam e se desfazem em um passo cada.

---

## 21. Riscos e mitigação

| Risco | Impacto | Mitigação |
|---|---|---|
| Mistura de dispositivo e símbolo | Alto | Entidades separadas e vínculo por `DeviceId` (§6) |
| Modelo discreto limitado | Alto | Escopo documentado; `StaticSeriesLoads` e `DynamicUnsupportedTopology` rejeitam o que não é suportado (§9.5) |
| Semântica temporal ambígua | Alto | Tick, commit e snapshot formalizados (§10); tabelas de ticks como testes (§11) |
| Referências instáveis de pinos | Alto | Chaves estruturais de terminal; rótulos derivados (§6.2) |
| Cruzamentos confundidos com junções | Alto | Junções explícitas (§6.6) |
| Escopo excessivo | Alto | MVP restrito; critérios de saída mensuráveis por marco |
| Estrutura antes do código (crates, ADRs, documentos) | Médio/alto | Módulos antes de crates (§5.3); 4 ADRs; código como fonte de verdade (§22) |
| Contratos divergirem do documento | Médio/alto | Documento corrigido na mesma PR que muda o contrato; goldens e testes de snapshot de códigos |
| GUI tardia, sem retorno visível | Médio/alto | Viewer read-only em M2a, logo após o M1 (§14.4) |
| Fixtures divergirem entre sistemas (fim de linha) | Médio | `.gitattributes` com `eol=lf`; serialização canônica; CI em três SOs (§15.1) |
| Forks de terceiros | Médio/alto | Preferir crates.io; fixar revisões só quando necessário (§5.5) |
| egui em canvas CAD grande | Médio/alto | Benchmarks, culling e cache (§14.5) |
| Portabilidade WASM | Médio | Núcleo sem I/O e sem relógio; `cargo check` WASM em CI (§5.4, §17.6) |
| Símbolos e licenças | Médio | Símbolos próprios e proveniência registrada (§18.2) |
| Arquivos `.ocircuit` hostis | Médio | Limites, validação estrita e testes de propriedade; fuzzing adiado (§19.2) |
| Uso como validação de segurança real | Alto | Avisos, documentação e limites explícitos (§2.6) |

---

## 22. Documentos normativos e ADRs

### 22.1 Documentos

Documentos do projeto, com papéis distintos:

- **`SIMULATION.md` (normativo):** semântica do tick, entradas, atraso de atuação, temporizadores, regras do solver, formato do trace.
- **`FORMAT.md` (normativo):** schema JSON, IDs, chaves de terminal, canonicalização, migrações e changelog de versões.
- Este documento é a visão e a justificativa; depois da v1.0 passa a ser o `ARCHITECTURE.md`. O roadmap fica em `ROADMAP.md`.

A partir do M0, os **tipos e testes reais** no código são a fonte de verdade dos contratos; os blocos Rust deste documento são ilustrativos.

### 22.2 ADRs obrigatórios

Escrever apenas decisões difíceis de reverter, antes de congelar os contratos. Cada ADR registra contexto, decisão, alternativas consideradas e consequências.

| ADR | Decisão |
|---|---|
| [ADR-001](adr/0001-identity-and-ids.md) | IDs `u64` com contador persistido, vínculo por `DeviceId`, separação Device/SymbolInstance/Terminal e chaves estruturais de terminal |
| [ADR-002](adr/0002-tick-and-commit.md) | Semântica do tick e do commit: entradas efetivas no mesmo tick, atraso mínimo de 1 tick, snapshot pré-commit, trace como log de mudanças, replay e checkpoint |
| [ADR-003](adr/0003-conductor-model.md) | Modelo de condutores (máscara de bits, um perfil por condutor), regras de conflito e limites do solver discreto |
| [ADR-004](adr/0004-canonical-json-format.md) | Formato JSON canônico sem ponto flutuante, versionamento, leitura em duas fases e recusa de schema mais novo |
| [ADR-005](adr/0005-m3-circuit-semantics.md) | M3: seções de alimentação, contatos comutadores e versões de modelo, intertravamento mecânico e enrolamentos do motor de seis terminais |
| [ADR-006](adr/0006-pdf-export.md) | Exportação PDF com `pdf-writer`: página do tamanho do desenho, bytes determinísticos; a fonte do texto foi alterada pela ADR-0012 |
| [ADR-007](adr/0007-control-transformer.md) | M4: transformador de comando, secundário vivo no tick seguinte (`Powered`), `OC-SEM-010` |
| [ADR-008](adr/0008-extension-libraries.md) | M4: bibliotecas de extensão só de dados (namespace, proveniência, desenho próprio) |
| [ADR-009](adr/0009-industrial-io.md) | M4: E/S industrial como servidor Modbus TCP; codec no núcleo, socket na CLI; limites de segurança |
| [ADR-010](adr/0010-opcua-mqtt-evaluation.md) | M4: avaliação de OPC UA (não implementado: só há implementações MPL-2.0) e MQTT (viável, adiado, desenho registrado) |
| [ADR-011](adr/0011-sheets-as-frames.md) | M5: folhas como molduras sobre a tela única, com tamanho de papel, zonas e carimbo; o simulador não muda |
| [ADR-012](adr/0012-pdf-font.md) | M5: subconjunto embutido da Liberation Mono no PDF, com um leitor mínimo de fonte no lugar de um crate sem manutenção |

---

## 23. Primeiro incremento executável

A primeira entrega de código (M1) é deliberadamente pequena: **três crates, a CLI e o `xtask`**.

1. `circuit-model`
2. `circuit-sim`
3. `circuit-project`
4. `apps/cli`
5. `xtask`

Não criar editor, renderer, exportador nem GUI nesta etapa. O viewer (M2a) começa depois da saída do M1.

### Critério técnico

Construir um cenário headless sobre `examples/direct-starter/command.ocircuit` (§15.2) que:

1. carrega o arquivo e confirma o round-trip byte a byte;
2. compila sem erros nem avisos;
3. cria o `Simulator` com a configuração padrão (1 ms, atrasos 1/1, `Halt`);
4. no tick 2 pressiona S1 e verifica a energização de K1;
5. no tick 6 solta S1 e verifica a retenção pelo contato de selo;
6. no tick 10 pressiona S2 (NF) e verifica a desenergização;
7. no tick 14 solta S2 e verifica que K1 **não** volta a energizar;
8. compara o trace com o golden;
9. roda no CI de Linux, Windows e macOS, com `cargo check` WASM do núcleo.

Trace esperado (formato ilustrativo; o normativo está em `SIMULATION.md`):

```text
# opencircuit trace v1
# fingerprint <hex64>
# tick_us=1000 pickup_ticks=1 dropout_ticks=1 fault_policy=halt
t=2  input   S1.pressed  0->1
t=2  load    K1.coil     on
t=3  signal  K1.active   0->1
t=6  input   S1.pressed  1->0
t=10 input   S2.pressed  0->1
t=10 load    K1.coil     off
t=11 signal  K1.active   1->0
t=14 input   S2.pressed  1->0
```

Leitura: a bobina energiza no tick da ação (2) e o contato de selo fecha um tick depois (3). A soltura de S1 (6) não altera nada, porque o selo mantém a bobina. Acionar S2 abre o caminho no mesmo tick (10) e o contato de selo abre no tick seguinte (11). Soltar S2 (14) fecha o NF, mas sem S1 nem selo a bobina permanece desligada.

A GUI será uma camada de apresentação desse mesmo cenário, nunca uma implementação paralela da lógica.

---

## 24. Decisões pendentes e status

Questões que são decisão de contrato, pois definem arquivo e testes, estão fechadas. As que dependem de medição permanecem em aberto.

| # | Questão | Status | Onde |
|---|---|---|---|
| 1 | Menor modelo de rede e fontes para o MVP | **Decidido**: máscara de condutores por grupo, um perfil por condutor. Validar nos goldens da partida direta e do reversor | §9 |
| 2 | Propriedades do motor simplificado | **Decidido**: `Stopped`/`Forward`/`Reverse` e `PhaseLoss`; sem grandezas físicas | §9.4 |
| 3 | Tick fixo | **Decidido**: padrão de 1 ms. Medição M3 ([`benchmarks.md`](benchmarks.md)): com 2.000 componentes, ~2,6 ms de cena e ~0,25 ms por tick num circuito que comuta a cada tick (~4 ms por frame em tempo real); 5.000 ainda cabem ao visualizar e simular; 10.000 excedem o frame (cena 13–15 ms, comutação constante ~23 ms por frame, arraste ~55 ms). Otimizações (cache de integridade, culling, buffers do solver) só quando um documento real precisar | §10.1, §14.5 |
| 4 | Roteamento automático do editor | **[a validar]**: primeira versão com traçado ortogonal e waypoints manuais; roteador automático depois | §6.6, M2 |
| 5 | Persistência e migração do catálogo | **Decidido**: recusa de campos e schema desconhecidos; `ModelRef` com versão e `ModelMigration` | §7, §15 |
| 6 | WASM | **Decidido** (M4): aplicação web compilada e verificada no CI, sem publicação no M4; núcleo verificado em CI desde o M1 | §17.6, M4 |
| 7 | Exportação | **Decidido**: SVG por escrita direta; PDF com `pdf-writer` após spike ([ADR-0006](adr/0006-pdf-export.md)) | §18.1 |
| 8 | Diagnósticos bloqueantes ou avisos | **Decidido**: tabela de códigos e severidades | §13.2 |
| 9 | Tolerância de tensão nos rótulos de carga | **[a validar]** (hoje igualdade inteira) | §9.4 |
| 10 | Atraso de atuação por dispositivo | **[adiado]** | §10.5 |
| 11 | Detecção automática de oscilação | **[adiado]** (diagnóstico opcional) | §10.5 |
| 12 | Rearme automático e curva do térmico | **[adiado]** | §10.3 |
| 13 | Catálogo orientado a dados e extensões | **Decidido** (M4): bibliotecas JSON só de dados, com namespace, proveniência e desenho próprio ([ADR-0008](adr/0008-extension-libraries.md)) | §7 |
| 14 | Intervalo de checkpoints | **[a validar]** | §17.4 |
| 15 | Política de falha padrão | **Decidido**: `Halt` (fail-closed); `Report` é opt-in | §10.6 |
| 16 | Esquema de códigos de diagnóstico | **Decidido**: `OC-<CAT>-NNN`, com severidade e coluna de trava | §13.2 |
| 17 | Seções de alimentação | **Decidido** (M3): condutor identificado por seção; ligar seções é curto; `PE` comum ([ADR-0005](adr/0005-m3-circuit-semantics.md)) | §6.7, §9.3 |
| 18 | Transformador de comando | **Decidido** (M4): secundário vivo no tick seguinte ao primário energizado, sinal `Powered` ([ADR-0007](adr/0007-control-transformer.md)) | §6.7 |
| 19 | Intertravamento mecânico | **Decidido** (M3): quebra antes de fechar; subida simultânea → nenhum sobe, aviso `OC-ELEC-007` ([ADR-0005](adr/0005-m3-circuit-semantics.md)) | §10 |

Respostas ainda abertas devem ser registradas em ADR ou neste documento.

---

## 25. Conclusão

A arquitetura do OpenCircuit é viável como projeto open source em Rust, e este documento a torna executável: contratos consistentes entre seções, estrutura inicial reduzida ao que o primeiro cenário exige, semântica fechada para o formato de arquivo e para os testes, e um retorno visual antecipado (viewer) sem entregar a lógica à interface.

As decisões mais importantes:

- manter Rust no núcleo e na UI, com egui/eframe só no desktop;
- separar dispositivo, símbolo e terminal, com vínculo por ID e chaves estruturais;
- compilar o documento (`CompileOutput`) em redes de cobre e resolver grupos por tick sobre uma máscara de condutores;
- manter a simulação headless, determinística e baseada em tempo virtual, com semântica de tick e commit explícita;
- recusar o que o solver discreto não representa, em vez de aproximar;
- persistir em JSON canônico, sem ponto flutuante, com leitura em duas fases e migrações testadas;
- começar com três crates, promover módulos a crates por gatilho objetivo e verificar as fronteiras em CI.

**A prioridade não é desenhar muitos símbolos. É provar que um circuito pequeno possui semântica correta, reproduzível e testável, e mostrá-lo funcionando cedo.**

O primeiro marco é o golden do comando da partida direta rodando nos três sistemas operacionais (M1). O segundo é a partida direta completa e o reversor, também sem GUI, ainda no M1. Depois vêm o viewer (M2a) e o editor (M2b). A partir desse núcleo, o OpenCircuit cresce sem transformar a interface gráfica em dona da lógica elétrica.

---

## Apêndice A — Checklist de aprovação da arquitetura

- [x] Device, SymbolInstance e Terminal separados; vínculo por `DeviceId`.
- [x] IDs `u64` com `next_id` persistido; chaves de terminal estruturais e rótulos derivados.
- [x] Junções explícitas e cruzamentos não ambíguos; `Wire` guarda só waypoints.
- [x] Conectividade em dois níveis (rede de cobre e grupo) documentada.
- [x] Condutores como máscara; um perfil por condutor; regras de conflito (§9.3).
- [x] Rótulos de tensão usados só para compatibilidade, nunca em cálculo.
- [x] Semântica do tick e do commit formalizada; snapshot coerente com o instante resolvido.
- [x] Atraso de atuação mínimo de 1 tick; TON e TOF com tabelas de ticks como testes.
- [x] `FaultPolicy` definida e testada (`Halt` por padrão; `Report` opt-in).
- [x] `CompileOutput` e diagnósticos com códigos `OC-<CAT>-NNN` estáveis.
- [x] Limites do solver documentados; `StaticSeriesLoads` e `DynamicUnsupportedTopology` implementados.
- [x] Regras de dependência verificadas no CI (`xtask`, `clippy.toml`, `cargo-deny`).
- [x] Formato JSON canônico, sem ponto flutuante, versionado, com leitura em duas fases e migrações.
- [x] CLI headless funcional (`validate`, `simulate`, `test`).
- [x] Golden da partida direta aprovado em Linux, Windows e macOS.
- [x] ADR-001 a ADR-004 escritos; `SIMULATION.md` e `FORMAT.md` criados.
- [x] Política de licenças e recursos gráficos documentada.

## Apêndice B — Referências técnicas

As versões e APIs devem ser verificadas novamente durante a implementação. Referências úteis:

- Rust: https://www.rust-lang.org/
- Cargo Book: https://doc.rust-lang.org/cargo/
- egui: https://docs.rs/egui/
- eframe: https://docs.rs/eframe/
- serde: https://serde.rs/
- serde_json: https://docs.rs/serde_json/
- cargo-deny: https://embarkstudios.github.io/cargo-deny/
- proptest: https://docs.rs/proptest/
- insta: https://insta.rs/
- Criterion: https://bheisler.github.io/criterion.rs/book/
- IEC 60617: https://std.iec.ch/iec60617

## Apêndice C — Glossário

| Termo | Definição |
|---|---|
| Rede de cobre | Conjunto de pinos e junções ligados por fios; estática, calculada na compilação |
| Grupo | União, em um tick, de redes de cobre ligadas por switches fechados |
| Condutor | Função elétrica (`L1`, `L2`, `L3`, `N`, `PE`, `DcPos`, `DcNeg`); presente em um grupo como bit de `ConductorSet` |
| Switch | Elemento compilado que liga duas redes de cobre quando fechado (contatos, fusível, elemento térmico) |
| Sinal efetivo | Valor de um sinal que vale durante um tick |
| Entrada externa | Sinal alterado por `InputAction` (`Actuated`, `Tripped`); efetivo no mesmo tick |
| Sinal derivado | Sinal calculado pela lógica (`CoilActive`, `TimerOut`); efetivo no tick seguinte ao cálculo |
| Commit | Passagem dos sinais derivados calculados para efetivos, ao fim do tick |
| Trace | Log ordenado das mudanças de sinais, cargas e falhas, base dos testes golden |
| Golden | Trace (e diagnósticos) esperado, versionado junto do exemplo |
| Fingerprint | Hash estável do circuito compilado, usado em checkpoints e traces |

---

**Fim do documento — OpenCircuit Arquitetura Técnica v0.3**