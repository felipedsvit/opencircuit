# OpenCircuit

## Documento Técnico Consolidado e Prospectivo

**Nome de trabalho:** OpenCircuit\
**Versão do documento:** 0.2.1 — revisão de contratos e escopo\
**Data:** 13/11/2026\
**Status:** Arquitetura consolidada para o primeiro vertical slice; contratos
de M1 congelados até o primeiro golden\
**Plataforma inicial:** Desktop — Linux, Windows e macOS\
**Plataforma futura:** WebAssembly\
**Linguagem:** Rust\
**Interface:** egui/eframe (M2)\
**Licença:** MIT OR Apache-2.0

------------------------------------------------------------------------

## 0. Mudanças desta revisão

Esta revisão corrige contradições de contrato e reduz a estrutura antes do
primeiro código executável. As decisões centrais são:

-   `compile()` devolve `CompileOutput { circuit: Option<_>, diagnostics }`;
    erros não escondem avisos e avisos não escondem um resultado parcial.
-   `SignalTable` e `DeviceSignals` pertencem à topologia, não à simulação.
-   O snapshot usa IDs de documento (`WireId`, `JunctionId`, `SymbolId`,
    `DeviceId`), não índices internos.
-   As nets de cobre são compiladas uma vez; por tick, apenas os switches
    fechados alteram a conectividade.
-   `Device` guarda `model_id` + `model_version`; terminais existem somente
    nos símbolos.
-   O documento persiste `next_id`; não usa `uuid`.
-   `DiagnosticCode` é um código string estável e extensível, não um enum
    fechado.
-   `Wire.route` guarda apenas waypoints intermediários.
-   O commit `current`/`next` é uma decisão central do motor, não uma
    consequência acidental.
-   O primeiro conjunto de crates é `circuit-model`, `circuit-sim`,
    `circuit-project`, `apps/cli` e `xtask`. Editor, commands, renderer e
    export entram com a GUI em M2/M3.
-   Entradas, TON/TOF, térmico, política pós-falha, trace e persistência
    estão definidos neste documento; não ficam como "decisão de protótipo".
-   Um viewer egui somente leitura vem antes do editor completo.
-   A partir de M0, os tipos reais em `circuit-model` e `circuit-sim` são a
    fonte de verdade. Este documento registra decisões e invariantes; não é
    uma cópia linha a linha do código.

------------------------------------------------------------------------

## 1. Resumo executivo

OpenCircuit é um editor e simulador de esquemas elétricos de comando, com
foco inicial em automação e painéis industriais. O objetivo é oferecer uma
alternativa open source a ferramentas como CADe SIMU, começando por
circuitos de comando e evoluindo de forma incremental para circuitos de
potência, documentação técnica, integração industrial e extensibilidade.

A decisão central continua sendo tratar o circuito como um modelo de
domínio independente da interface. O editor apresenta e altera esse modelo;
o compilador o transforma em uma representação executável; o simulador
calcula os estados; e a renderização apresenta o documento e os resultados
da simulação.

**Princípio arquitetural:** OpenCircuit não deve ser um editor de desenhos
que tenta inferir eletricidade a partir de pixels. Deve ser um modelo de
circuito que possui uma representação gráfica.

A versão 0.2.1 consolida três decisões que sustentam o restante da
arquitetura:

1.  **Device ≠ SymbolInstance ≠ Terminal.** Um contator tem uma identidade
    física, mas várias representações gráficas (bobina, contato auxiliar,
    contatos principais). O terminal pertence ao símbolo, não ao
    dispositivo.
2.  **Circuito compilado com nets de cobre estáticas.** Fios e junções
    definem a conectividade de cobre uma única vez. A cada tick, apenas os
    switches fechados unem nets. Isso remove a ambiguidade entre "net" e
    "net dinâmica" e evita refazer o Union-Find dos fios a cada tick.
3.  **Commit síncrono `current`/`next`.** A rede é resolvida com os sinais
    efetivos do tick; a lógica calcula os sinais do tick seguinte; o
    snapshot representa a rede que foi realmente resolvida. Esse commit é
    o que quebra o laço bobina → contato → bobina do circuito de selo.

O primeiro incremento executável não é a GUI. É um vertical slice headless
de uma partida direta, incluindo Q1, F1, S1, S2, K1 e motor simplificado,
com trace golden em forma de log de mudanças. O viewer egui somente leitura
vem depois do motor e antes do editor completo.

O MVP será um editor desktop capaz de desenhar, salvar, carregar e simular
uma partida direta de motor, incluindo circuito de selo, contatos
auxiliares, proteção térmica e sinalização. A arquitetura deve permitir
ampliar o catálogo sem acoplar o núcleo à GUI.

------------------------------------------------------------------------

## 2. Objetivos e escopo

### 2.1 Objetivos

1.  Criar um editor de esquemas elétricos industriais.
2.  Simular circuitos de comando de forma determinística e reproduzível.
3.  Oferecer diagnóstico de erros de ligação e de configuração.
4.  Manter o núcleo independente de interface gráfica e sistema
    operacional.
5.  Permitir automação de testes por CLI.
6.  Disponibilizar um formato de projeto documentado e versionado.
7.  Manter o projeto aberto a contribuições, extensões e integrações
    futuras.

### 2.2 Plataformas

  -----------------------------------------------------------------------
  Plataforma              Fase                    Observação
  ----------------------- ----------------------- -----------------------
  Linux                   MVP                     Aplicação desktop
                                                  nativa

  Windows                 MVP                     Aplicação desktop
                                                  nativa

  macOS                   MVP                     Aplicação desktop
                                                  nativa

  WebAssembly             Posterior               Após validação de
                                                  dependências e
                                                  experiência de uso
  -----------------------------------------------------------------------

A compatibilidade WASM será tratada como requisito de portabilidade do
núcleo desde o início, mas não como compromisso de entregar a aplicação
web no MVP.

### 2.3 Dentro do MVP

-   Fontes de alimentação de comando e referências de condutores.
-   Botões NA e NF, momentâneos.
-   Interruptor com retenção genérico para Q1, seletora e emergência.
-   Contatos auxiliares NA e NF.
-   Bobinas de contatores e relés.
-   Temporizadores TON e TOF.
-   Relé térmico com latch e reset manual (F1).
-   Lâmpadas e buzzer.
-   Representação simplificada de motor trifásico como carga.
-   Viewer egui somente leitura para carregar, animar e inspecionar um
    projeto.
-   Editor com seleção, inserção, movimentação, rotação e exclusão.
-   Fios, junções e roteamento básico.
-   Snap-to-grid, zoom e pan.
-   Undo e redo.
-   Salvar e carregar projeto.
-   Simulação interativa.
-   Diagnósticos estruturais, semânticos e elétricos básicos.
-   CLI headless para executar cenários e testes.

### 2.4 Fora do MVP

-   Simulação analógica geral.
-   Cálculo rigoroso de tensão, corrente, impedância e transitórios.
-   Pneumática e hidráulica.
-   PLC real e execução de programas Ladder completos.
-   Modbus, OPC UA e MQTT.
-   Plugins WASM.
-   Colaboração em tempo real.
-   Importação de arquivos proprietários do CADe SIMU.
-   Certificação ou validação normativa de instalações reais.

### 2.5 Limite de responsabilidade

O OpenCircuit será uma ferramenta de desenho, aprendizado e simulação
preliminar. Seus resultados não substituem projeto elétrico profissional,
análise de risco, ensaios, comissionamento, validação de proteção ou
atendimento às normas aplicáveis. O software não deverá alegar certificar
instalações ou garantir segurança operacional.

------------------------------------------------------------------------

## 3. Princípios arquiteturais

1.  **Domínio independente:** nenhum crate do núcleo depende de egui,
    eframe ou APIs de sistema operacional.
2.  **Modelo explícito:** dispositivos, símbolos, terminais, fios,
    junções, fontes e entradas são entidades distintas.
3.  **Compilação do circuito:** o documento é compilado para uma
    representação executável antes da simulação.
4.  **Determinismo:** com o mesmo circuito compilado, estado inicial,
    sequência de entradas e configuração temporal, o runtime deve
    produzir o mesmo trace lógico.
5.  **Tempo virtual:** o motor não lê o relógio do sistema.
6.  **Commit síncrono:** sinais derivados de lógica são confirmados para o
    tick seguinte; entradas externas são aplicadas na fronteira do tick.
7.  **Snapshot em IDs de documento:** o snapshot não expõe índices
    internos e representa um único instante lógico.
8.  **Nets de cobre estáticas, switches dinâmicos:** a conectividade de
    fios e junções é compilada uma vez; apenas os switches variam por
    tick.
9.  **Falha elétrica é falha fechada:** curto e falta para PE interrompem
    a execução após o snapshot do tick, em vez de continuar produzindo
    estado inválido.
10. **Diagnóstico estruturado:** erros têm códigos string estáveis e
    referências aos elementos envolvidos.
11. **Testabilidade headless:** toda a simulação pode ser executada sem
    GUI.
12. **Evolução incremental:** o MVP não deve simular grandezas físicas que
    o solver ainda não calcula; fronteiras de crate só são criadas quando
    um segundo consumidor real as justifica.

------------------------------------------------------------------------

## 4. Arquitetura geral

### 4.1 Fluxo de dados

``` text
                    .ocircuit (JSON)
                          │
                          ▼
                 ┌──────────────────┐
                 │  circuit-project │
                 │ load / save /    │
                 │ migrations       │
                 └────────┬─────────┘
                          │ Schematic
                          ▼
                 ┌──────────────────┐
                 │  circuit-model   │
                 │ IDs, Device,     │
                 │ Symbol, Wire,    │
                 │ Catalog, Inputs, │
                 │ Diagnostics,     │
                 │ SimSnapshot      │
                 └────────┬─────────┘
                          │ &Schematic + &ComponentCatalog
                          ▼
                 ┌──────────────────┐
                 │   circuit-sim    │
                 │ compile          │
                 │  → CompileOutput │
                 └────────┬─────────┘
                          │ Option<CompiledCircuit>
                          ▼
                 ┌──────────────────┐
                 │   circuit-sim    │
                 │ electrical solve │
                 │ logic / timers   │
                 │ runtime / trace  │
                 └────────┬─────────┘
                          │ step / step_n
                          ├──────────────► ChangeLog (opcional)
                          │
                          ▼
                 ┌──────────────────┐
                 │    snapshot()    │
                 │  SimSnapshot     │
                 │  (circuit-model) │
                 └────────┬─────────┘
                          │
              ┌───────────┴────────────┐
              ▼                        ▼
     ┌──────────────────┐     ┌──────────────────┐
     │    apps/cli      │     │  apps/desktop    │
     │ JSON / golden    │     │  viewer (M2)     │
     └──────────────────┘     │  editor (M2b)    │
                              └──────────────────┘
```

O diagrama é conceitual. `circuit-sim` contém os módulos `topology`,
`electrical`, `logic`, `runtime` e `trace`; não há um crate por camada
enquanto não houver um segundo consumidor que justifique a separação.

### 4.2 Separação de responsabilidades

-   **Modelo:** descreve o circuito, sua geometria, o catálogo, as
    entradas e os contratos neutros de snapshot.
-   **Compilação:** valida o documento e produz a representação executável
    com índices densos, nets de cobre, switches, fontes, cargas e tabela de
    sinais.
-   **Electrical:** resolve conectividade ideal, máscaras de condutores,
    curtos e energização discreta das cargas.
-   **Logic:** avalia bobinas, contatos, temporizadores, térmico e
    entradas, produzindo os sinais do próximo tick.
-   **Runtime:** orquestra o tick, mantém `current`/`next`, aplica a
    política pós-falha e produz snapshots e trace.
-   **Project:** serializa, desserializa, valida e migra documentos.
-   **CLI:** executa validação, simulação e testes sem interface.
-   **Viewer/editor:** consomem o documento e o snapshot; não acessam
    índices internos do simulador.

------------------------------------------------------------------------

## 5. Workspace Rust

### 5.1 Estrutura alvo

``` text
opencircuit/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── README.md
├── LICENSE-APACHE
├── LICENSE-MIT
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── ARCHITECTURE.md
├── FORMAT.md
├── SIMULATION.md
├── ROADMAP.md
├── apps/
│   ├── cli/
│   └── desktop/                 # M2
├── crates/
│   ├── circuit-model/
│   │   └── src/
│   │       ├── ids.rs
│   │       ├── document.rs
│   │       ├── catalog.rs
│   │       ├── conductor.rs
│   │       ├── diagnostics.rs
│   │       ├── inputs.rs
│   │       └── snapshot.rs
│   ├── circuit-sim/
│   │   └── src/
│   │       ├── topology/
│   │       ├── electrical/
│   │       ├── logic/
│   │       ├── runtime/
│   │       └── trace.rs
│   ├── circuit-project/
│   ├── circuit-renderer/        # M2
│   ├── circuit-editor/          # M2b
│   ├── circuit-commands/        # M2b
│   └── circuit-export/          # M3
├── assets/
│   ├── symbols/
│   ├── icons/
│   └── themes/
├── examples/
│   ├── direct-starter/
│   ├── forward-reverse/
│   └── star-delta/              # cenário de referência futuro
├── tests/
│   ├── fixtures/
│   └── integration/
└── xtask/
```

Os diretórios de M2/M3 existem como placeholders até o marco correspondente.
Não criar `Cargo.toml` nem adicionar o crate ao workspace antes de haver
`src/lib.rs` real e um consumidor.

### 5.2 Responsabilidade dos crates

  -----------------------------------------------------------------------
  Crate                               Responsabilidade
  ----------------------------------- -----------------------------------
  `circuit-model`                     IDs, documento, catálogo,
                                      condutores, diagnósticos, entradas
                                      e tipos de snapshot. Sem solver, sem
                                      I/O, sem GUI.

  `circuit-sim`                       Compilação, nets de cobre, solver
                                      elétrico, lógica, temporizadores,
                                      térmico, runtime, política de falha
                                      e change log.

  `circuit-project`                   JSON, `schema_version`, validação,
                                      migrações e round-trip.

  `apps/cli`                          Validação, simulação, execução de
                                      goldens e saída JSON. Depende de
                                      `circuit-model`, `circuit-sim` e
                                      `circuit-project`.

  `xtask`                             Verificação das regras de
                                      arquitetura e automação do
                                      repositório.

  `circuit-renderer` (M2)             Cena neutra a partir do documento e
                                      do snapshot. Não depende de
                                      `circuit-sim`.

  `circuit-editor` (M2b)              Seleção, hit-test, roteamento e
                                      estado de interação.

  `circuit-commands` (M2b)            Comandos transacionais e undo/redo.

  `circuit-export` (M3)               SVG e PDF a partir da cena neutra.
  -----------------------------------------------------------------------

### 5.3 Regras de dependência

A regra é uma allowlist por crate, não uma lista de proibições. O
`xtask check-architecture` deve falhar se um crate depender de algo fora da
lista.

``` text
circuit-model       → nenhum crate do workspace
circuit-sim         → circuit-model
circuit-project     → circuit-model
circuit-renderer    → circuit-model
circuit-commands    → circuit-model
circuit-editor      → circuit-model, circuit-commands
circuit-export      → circuit-model, circuit-renderer
apps/cli            → circuit-model, circuit-sim, circuit-project
apps/desktop        → circuit-model, circuit-sim, circuit-project,
                      circuit-renderer, circuit-editor, circuit-commands,
                      circuit-export (quando existirem)
```

Regras obrigatórias:

1.  `circuit-model` não depende de nenhum crate do workspace.
2.  `circuit-sim` não depende de project, editor, renderer, commands,
    export ou apps.
3.  `circuit-project` não depende de `circuit-sim`.
4.  `circuit-renderer`, `circuit-editor` e `circuit-commands` não dependem
    de `circuit-sim`; consomem `Schematic` e `SimSnapshot` de
    `circuit-model`.
5.  Nenhum crate `circuit-*` depende de egui ou eframe. Código de UI fica
    em `apps/desktop`.
6.  Adaptadores de I/O não introduzem sockets, relógios ou dependências de
    sistema operacional no núcleo.
7.  O CI verifica as regras com `cargo metadata` por meio de
    `cargo run -p xtask -- check-architecture`. `cargo-deny` complementa a
    verificação com políticas de dependências, fontes e licenças; não
    substitui a validação das camadas internas.

### 5.4 Cargo.toml de referência

As versões devem ser resolvidas e testadas no momento da implementação. O
exemplo abaixo é uma base mínima, não uma garantia de compatibilidade:

``` toml
[workspace]
resolver = "3"
members = [
    "apps/cli",
    "crates/circuit-model",
    "crates/circuit-sim",
    "crates/circuit-project",
    "xtask",
]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.85"

[workspace.dependencies]
circuit-model = { path = "crates/circuit-model", version = "0.0.1" }
circuit-sim = { path = "crates/circuit-sim", version = "0.0.1" }
circuit-project = { path = "crates/circuit-project", version = "0.0.1" }

serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
clap = { version = "4", features = ["derive"] }

proptest = "1"
insta = "1"
tempfile = "3"
```

`egui` e `eframe` não entram no workspace de M1. Eles são adicionados em
M2a, quando `apps/desktop` existir.

Observações:

-   `uuid` não é usado. Os IDs são `u64` alocados por um contador
    `next_id` persistido. Isso evita a configuração extra de `getrandom`
    em `wasm32` e mantém o formato legível.
-   `fixedbitset` e `petgraph` não entram na base inicial. A tabela de
    sinais pode usar `Vec<bool>`; o Union-Find pode ser implementado
    diretamente. Se o profiling justificar, a dependência entra depois.
-   `anyhow` é opcional e, se usado, fica restrito a `apps/cli`. As
    bibliotecas usam erros tipados com `thiserror`.
-   `egui`/`eframe` 0.37.0 foram verificados com `cargo search` em
    13/11/2026. A página do docs.rs pode estar atrasada. No início do M2a,
    usar `cargo add egui eframe` e fixar a versão escolhida pelo
    resolvedor; não transcrever o número sem revalidar.
-   `proptest`, `insta` e `tempfile` são dependências de teste.
-   `criterion` entra apenas quando houver benchmarks reais (M2/M3).

**Nota sobre MSRV:** `rust-version` deve ser ajustado depois de verificar a
versão mínima exigida pelo conjunto real de dependências e pelos alvos
suportados. Fixar `rust-toolchain.toml`, versionar `Cargo.lock` e executar
CI com `--locked`.

### 5.5 Política de forks e referência CADCraft

A configuração inicial deve preferir crates do crates.io. O CADCraft é uma
referência útil de editor CAD, mas o manifesto público dele usa
`[patch.crates-io]` para `egui`, `winit`, `krilla`, `hayro-syntax` e
`hayro-jbig2`. Isso significa que ele não é um exemplo de "crates.io puro".

Antes de copiar qualquer configuração:

1.  Inspecionar o `Cargo.toml` e o `Cargo.lock` do CADCraft na revisão
    analisada.
2.  Identificar por que cada patch existe: IME, acessibilidade, DPI,
    renderização de PDF, correção de `winit`, etc.
3.  Reproduzir o problema no OpenCircuit antes de adotar o fork.
4.  Fixar a revisão e documentar licença, manutenção e estratégia de
    atualização.
5.  Testar a combinação completa da família egui/winit; não misturar tipos
    de upstream e fork na mesma aplicação.

Fork só é adotado quando houver problema reproduzível, necessidade que o
upstream não atende, revisão de licença e segurança, commit imutável
fixado, testes que demonstrem o benefício e responsável pela manutenção.
Isso é um spike de M2, não uma decisão de M1.

### 5.6 Estado atual e migração do M0

> **Estado da implementação em 13/11/2026.** O M0 entregou
> `circuit-domain` e `circuit-diagnostics`, com IDs, dispositivo, símbolo,
> terminal, fio, junção, condutores, perfil de alimentação e diagnósticos
> estruturais. O primeiro passo do M1 consolida esses dois crates em
> `circuit-model` e cria `circuit-sim`, `circuit-project` e `apps/cli`.

A migração é deliberada:

-   `circuit-domain` e `circuit-diagnostics` viram módulos de
    `circuit-model`; não há razão para manter dois crates com um único
    consumidor.
-   `circuit-topology`, `circuit-electrical`, `circuit-logic` e
    `circuit-simulation` viram módulos de `circuit-sim`. Os diretórios
    placeholder correspondentes devem ser removidos quando o módulo real
    existir.
-   `circuit-components` vira o módulo `catalog` de `circuit-model`; o
    catálogo embutido do M1 é código Rust, não um crate separado.
-   `circuit-io` não existe no alvo. `circuit-project` cuida do formato e
    `apps/*` cuida de paths, leitura e escrita atômica.
-   `circuit-commands`, `circuit-editor`, `circuit-renderer` e
    `circuit-export` só entram quando a GUI começar.
-   O `circuit-project` entra no M1 porque o critério de saída carrega um
    arquivo `.ocircuit` e exige salvar/reabrir.

Essa consolidação é uma aplicação do princípio de monolito modular:
fronteira como módulo primeiro; crate quando um segundo consumidor
justificar.

------------------------------------------------------------------------

## 6. Modelo de domínio

### 6.1 Separar dispositivo, símbolo e terminal

Um dispositivo representa o equipamento lógico ou físico. Um símbolo é uma
representação gráfica desse dispositivo. Um terminal é um ponto de conexão
estável, declarado pelo modelo do símbolo.

Essa separação evita que uma bobina e seus contatos auxiliares sejam
tratados como dispositivos sem relação entre si.

``` rust
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WireId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JunctionId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SupplyId(pub u64);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReferenceDesignator(pub String);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TerminalKey(pub String);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolModelId(pub String);
```

Exemplo conceitual:

``` text
Device K1  (modelo "contactor.3p", versão 1)
├── Symbol: coil K1                 (A1, A2)
├── Symbol: auxiliary NO contact K1 (13, 14)
├── Symbol: auxiliary NC contact K1 (21, 22)
└── Symbol: main contacts K1        (1, 2, 3, 4, 5, 6)
```

A referência `K1` pode aparecer em diversos símbolos associados ao mesmo
`DeviceId`. A duplicidade deve ser detectada entre dispositivos distintos,
não entre símbolos do mesmo dispositivo.

### 6.2 Referências a terminais

``` rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PinRef {
    pub symbol_id: SymbolId,
    pub terminal: TerminalKey,
}
```

Exemplos de chaves estáveis:

-   `A1`, `A2`
-   `13`, `14`
-   `21`, `22`
-   `U1`, `V1`, `W1`
-   `U2`, `V2`, `W2`

O arquivo de projeto armazena chaves estáveis. O compilador resolve essas
chaves para índices densos internos. Um terminal só existe se o modelo do
símbolo o declarar; o compilador nunca inventa pinos posicionais.

### 6.3 Dispositivo e identidade de modelo

O documento não guarda um enum de comportamento. Guarda a referência ao
modelo do catálogo e a versão:

``` rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModelId(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    pub id: DeviceId,
    pub reference: ReferenceDesignator,
    pub model_id: ModelId,
    pub model_version: u16,
    pub parameters: BTreeMap<ParameterKey, ParameterValue>,
}
```

O catálogo resolve `(model_id, model_version)` para um `ComponentModel`.
Os terminais e o comportamento elétrico vêm do modelo e dos símbolos, não
de um enum fechado no documento. `DeviceKind` continua existindo como
classificação de catálogo (contator, relé, temporizador, etc.), mas não é
identidade persistida nem fonte de terminais.

`parameters` existe porque um TON/TOF precisa de preset por dispositivo e
um modelo futuro pode precisar de outros valores. O mapa é `BTreeMap` para
ordem determinística. O catálogo define chaves, tipos, defaults e
validação.

### 6.4 Papéis e instâncias de símbolo

``` rust
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolRole {
    Coil,
    ContactNo,
    ContactNc,
    Changeover,
    Lamp,
    Motor,
    Buzzer,
    SupplyTerminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SymbolControl {
    Device,
    Input(InputKey),
    Thermal,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolInstance {
    pub id: SymbolId,
    pub device_id: DeviceId,
    pub symbol_model_id: SymbolModelId,
    pub position: GridPoint,
    pub rotation: QuarterTurn,
}
```

O modelo do símbolo declara o papel, o controle, os terminais e a
geometria. A instância guarda apenas a escolha do modelo, a posição e a
rotação. Dois contatos NA do K1 são duas instâncias independentes; podem
usar o mesmo `symbol_model_id` ou modelos diferentes.

O controle é explícito:

-   `Device`: contato auxiliar de contator/relé, controlado pelo estado de
    atuação do próprio dispositivo.
-   `Input`: contato de botão ou interruptor, controlado por uma entrada
    do mesmo dispositivo.
-   `Thermal`: contato NF do relé térmico, controlado pelo latch.
-   `None`: bobina, lâmpada, motor, buzzer ou terminal de fonte.

No M1, `Changeover` é catalogado, mas o compilador emite
`OC-MOD-001 unsupported-topology` se for usado. O suporte completo entra
com o modelo de contatos reversíveis em M3.

### 6.5 Posição e rotação

``` rust
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GridPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum QuarterTurn {
    #[default]
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}
```

As coordenadas são unidades lógicas do documento, nunca pixels de tela. A
posição absoluta de um terminal é derivada de `SymbolInstance.position`,
`SymbolInstance.rotation` e do offset declarado no modelo do símbolo.

### 6.6 Fios, junções e cruzamentos

A geometria e a conectividade devem ser representadas explicitamente.

``` rust
pub struct Junction {
    pub id: JunctionId,
    pub position: GridPoint,
}

pub enum ConnectionEndpoint {
    Terminal(PinRef),
    Junction(JunctionId),
}

pub struct Wire {
    pub id: WireId,
    pub from: ConnectionEndpoint,
    pub to: ConnectionEndpoint,
    pub route: Vec<GridPoint>,
}
```

`route` guarda somente os waypoints intermediários. Os pontos inicial e
final são derivados dos endpoints; não são duplicados no arquivo. Mover um
símbolo invalida a posição derivada do pino, mas não corrompe a lista de
waypoints.

Dois fios que apenas se cruzam visualmente não devem ser conectados
automaticamente. A conexão exige uma junção explícita ou uma regra de
interseção claramente definida e testada. Uma junção com menos de dois
endpoints de fio é um diagnóstico estrutural, não uma conexão implícita.

### 6.7 Condutores e alimentação

O enum `Conductor` identifica a função do condutor, não uma grandeza
elétrica:

``` rust
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Conductor {
    L1 = 1 << 0,
    L2 = 1 << 1,
    L3 = 1 << 2,
    Neutral = 1 << 3,
    ProtectiveEarth = 1 << 4,
    DcPositive = 1 << 5,
    DcNegative = 1 << 6,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ConductorMask(pub u8);
```

O bitmask é o menor modelo que atende ao MVP: uma net dinâmica carrega o
conjunto de condutores presentes nela. `u8` comporta os sete condutores
atuais e deixa um bit livre. O bitmask não representa tensão, corrente ou
potencial instantâneo; representa quais funções de condutor alcançam a
net.

`ProtectiveEarth` não é uma fonte de alimentação e não deve energizar
cargas. Neutro e PE não são intercambiáveis. A ligação entre neutro e PE
depende do esquema de aterramento e do ponto de equipotencialização
configurado; no M1, uma net que contenha PE e qualquer outro condutor gera
`OC-ELEC-002 protective-earth-fault`, e o projeto deve declarar a ligação
no ponto de origem se ela for intencional.

A configuração de alimentação é explícita:

``` rust
pub struct SupplyProfile {
    pub id: SupplyId,
    pub kind: SupplyKind,
    pub nominal_voltage_mv: u32,
    pub frequency_millihz: Option<u32>,
    pub phase_sequence: Option<PhaseSequence>,
    pub conductors: Vec<Conductor>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SupplyKind {
    Dc,
    SinglePhase,
    ThreePhase,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhaseSequence {
    L1L2L3,
    L1L3L2,
}
```

`conductors` declara quais condutores o perfil fornece. Para DC, tipicamente
`[DcPositive, DcNegative]`; para monofásico, `[L1, Neutral]`; para
trifásico, `[L1, L2, L3]` e, se aplicável, `Neutral` e `ProtectiveEarth`.
O compilador valida que todo terminal de fonte usa um condutor declarado
pelo perfil.

`nominal_voltage_mv` e `frequency_millihz` **não entram no solver**. Servem
apenas para a checagem de compatibilidade da bobina/carga contra a fonte,
feita na camada lógica. O solver trabalha com conectividade e condutores.

### 6.8 Documento e `next_id`

``` rust
pub struct Schematic {
    pub schema_version: u32,
    pub next_id: u64,
    pub name: String,
    pub grid_unit: u32,
    pub devices: Vec<Device>,
    pub symbols: Vec<SymbolInstance>,
    pub junctions: Vec<Junction>,
    pub wires: Vec<Wire>,
    pub supplies: Vec<SupplyProfile>,
}
```

`next_id` é um contador global persistido. Toda entidade nova consome o
próximo valor e o contador avança; IDs nunca são reutilizados depois de
uma exclusão. Na carga, `next_id` deve ser maior que o maior ID existente;
caso contrário, o documento é inválido (`OC-STR-006 invalid-next-id`).

O contador global evita colisões entre tipos de entidade e elimina a
necessidade de `uuid` no núcleo. O formato continua legível e determinístico.

### 6.9 Entradas

Entradas são declaradas pelo catálogo e referenciadas por uma chave estável
dentro do dispositivo:

``` rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InputKey(pub String);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InputId {
    pub device: DeviceId,
    pub key: InputKey,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InputBehavior {
    Momentary,
    Maintained,
    Pulse,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Inputs {
    active: BTreeSet<InputId>,
}
```

O catálogo declara, por modelo de componente:

``` rust
pub struct InputDefinition {
    pub key: InputKey,
    pub label: String,
    pub behavior: InputBehavior,
}
```

Semântica:

-   **Momentary:** botão NA/NF. Fica ativo enquanto pressionado. O contato
    correspondente fecha/abre no mesmo tick em que a entrada é aplicada.
-   **Maintained:** interruptor com retenção. Q1, seletora e emergência
    usam o mesmo mecanismo. A UI mantém o estado alternado e o reporta
    como ativo/inativo a cada tick.
-   **Pulse:** gatilho de borda. O térmico usa `trip` e `reset` como
    pulsos; a ação ocorre uma vez na transição de inativo para ativo.

`Inputs` é um conjunto de entradas ativas, não um mapa de tensões. A
diferença entre momentâneo, mantido e pulso está no comportamento declarado
pelo catálogo e na UI, não em tipos diferentes de sinal no motor. O runtime
compara o conjunto atual com o anterior para detectar bordas de pulso.

Uma partida direta realista usa:

``` text
Q1.closed   maintained   interruptor geral
S1.press    momentary    botão LIGA (NA)
S2.press    momentary    botão DESLIGA (NF)
F1.trip     pulse        teste manual do térmico
F1.reset    pulse        reset manual do térmico
```

------------------------------------------------------------------------

## 7. Catálogo de componentes

O catálogo é a fonte de verdade para modelos, terminais, parâmetros,
entradas, comportamento e geometria. O documento referencia o catálogo por
`ModelId` + `model_version`; não duplica terminais no dispositivo.

### 7.1 Modelo de componente

``` rust
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceKind {
    Contactor,
    Relay,
    Timer(TimerKind),
    ThermalOverload,
    Pushbutton,
    MaintainedSwitch,
    Motor,
    Lamp,
    Buzzer,
    Supply,
}

pub struct ComponentModel {
    pub id: ModelId,
    pub version: u16,
    pub kind: DeviceKind,
    pub inputs: Vec<InputDefinition>,
    pub parameters: Vec<ParameterDefinition>,
    pub symbols: Vec<SymbolModel>,
}

pub struct ComponentCatalog {
    models: BTreeMap<(ModelId, u16), ComponentModel>,
}

impl ComponentCatalog {
    pub fn builtin() -> Self;
    pub fn resolve(&self, id: &ModelId, version: u16)
        -> Option<&ComponentModel>;
}
```

`ComponentCatalog::builtin()` é o catálogo embutido do M1. O mapa é
`BTreeMap` para ordem determinística. Um catálogo externo só entra quando
houver um segundo consumidor real (ver §24.5).

`DeviceKind` é uma classificação de catálogo e uma família de comportamento.
Não é persistido no documento e não define terminais. O documento guarda
apenas `model_id` + `model_version`.

### 7.2 Modelo de símbolo e terminais

``` rust
pub struct SymbolGeometry {
    pub width: i32,
    pub height: i32,
    pub terminals: Vec<TerminalOffset>,
}

pub struct TerminalOffset {
    pub terminal: TerminalKey,
    pub offset: GridPoint,
}

pub struct SymbolModel {
    pub id: SymbolModelId,
    pub role: SymbolRole,
    pub control: SymbolControl,
    pub terminals: Vec<TerminalDefinition>,
    pub geometry: SymbolGeometry,
    pub load: Option<LoadModel>,
    pub standard_reference: Option<String>,
}

pub struct TerminalDefinition {
    pub key: TerminalKey,
    pub label: String,
    pub required: bool,
    pub conductor: Option<Conductor>,
}

pub struct LoadModel {
    pub kind: LoadKind,
    pub supply_kind: SupplyKind,
    pub required_conductors: Vec<Conductor>,
    pub rating: LoadRating,
}

pub struct LoadRating {
    pub voltage_min_mv: u32,
    pub voltage_max_mv: u32,
    pub frequency_min_millihz: Option<u32>,
    pub frequency_max_millihz: Option<u32>,
}
```

Os terminais pertencem ao modelo do símbolo, não ao dispositivo. Um contato
NA e um contato NF do mesmo K1 podem ter modelos de símbolo distintos, cada
um com seus próprios terminais e geometria. A bobina tem `A1`/`A2`; o
contato auxiliar tem `13`/`14`; os contatos principais têm `1..6`.

`TerminalDefinition.conductor` só é preenchido para terminais de fonte
(`L1`, `N`, `PE`, `DC+`, `DC-`) ou para terminais que representam um
condutor fixo. Terminais de contato, bobina e motor não declaram condutor;
o condutor é determinado pela net à qual o terminal pertence.

Quando `SymbolModel.load` existe, a ordem dos terminais de carga no modelo
do símbolo corresponde à ordem de `LoadModel.required_conductors`. Para uma
bobina, os terminais são `A1`/`A2`; para o motor, `U1`/`V1`/`W1`.

### 7.3 Parâmetros

``` rust
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ParameterKey(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParameterValue {
    Bool(bool),
    Integer(i64),
    DurationUs(u64),
    Text(String),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ParameterKind {
    Bool,
    Integer,
    Duration,
    Text,
}

pub struct ParameterRange {
    pub min: ParameterValue,
    pub max: ParameterValue,
}

pub struct ParameterDefinition {
    pub key: ParameterKey,
    pub label: String,
    pub kind: ParameterKind,
    pub default: ParameterValue,
    pub range: Option<ParameterRange>,
}
```

O TON/TOF usa `preset` como `DurationUs`. O catálogo valida tipo, faixa e
default. O documento guarda o valor por dispositivo em
`Device.parameters`. Um parâmetro ausente usa o default do catálogo; um
parâmetro desconhecido ou fora da faixa é erro semântico.

### 7.4 Versionamento

O catálogo é versionado por modelo. Alterar o significado ou a chave de um
terminal exige uma nova `model_version` e uma migração do documento, ou uma
compatibilidade explícita. A versão do modelo do componente é distinta da
versão geral do documento.

Regras:

-   O documento referencia uma versão exata; não usa "latest".
-   Se a versão não existir, o carregamento/compilação falha com
    `OC-SEM-002 unknown-model` ou `OC-SEM-003 model-version-mismatch`.
-   Um modelo pode declarar aliases de terminal somente se a migração for
    documentada e testada.
-   Símbolos e modelos de terceiros exigem proveniência e licença
    registradas.

------------------------------------------------------------------------

## 8. Compilação da topologia

### 8.1 Pipeline

``` text
Schematic
   │
   ├── validar schema_version, next_id e IDs únicos
   ├── resolver model_id + model_version no catálogo
   ├── validar símbolos, terminais e parâmetros
   ├── resolver endpoints de fio e junções
   ├── construir nets de cobre (fios + junções)
   ├── anexar fontes, cargas e switches às nets de cobre
   ├── criar a tabela de sinais
   ├── atribuir índices densos estáveis
   └── emitir CompileOutput
```

A compilação **não** aplica estados de switch. Ela produz a conectividade de
cobre estática, as fontes, as cargas, os switches e a tabela de sinais. A
conectividade dinâmica é responsabilidade do solver, a cada tick.

### 8.2 Contrato de compilação

``` rust
pub struct CompileOutput {
    pub circuit: Option<CompiledCircuit>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn compile(
    schematic: &Schematic,
    catalog: &ComponentCatalog,
) -> CompileOutput;
```

Regras:

-   `compile` nunca devolve `Result`. Sempre devolve `CompileOutput`.
-   `circuit` é `Some` quando não há diagnóstico de severidade `Error`.
-   Diagnósticos `Warning` e `Info` podem coexistir com um circuito
    compilado.
-   Se houver `Error`, `circuit` é `None`; não se expõe um circuito
    parcialmente indexado.
-   Os diagnósticos são ordenados por código, sujeito e relacionados antes
    de retornar.

Isso corrige a contradição da v0.2: avisos não desaparecem em caso de
sucesso, e o resultado parcial não desaparece em caso de erro.

### 8.3 Índices internos

Usar newtypes para impedir mistura acidental de índices:

``` rust
pub struct PinIx(pub u32);
pub struct CopperNetIx(pub u32);
pub struct DynamicNetIx(pub u32);
pub struct SignalIx(pub u32);
pub struct DeviceIx(pub u32);
pub struct LoadIx(pub u32);
```

`NetIx` deixa de existir como nome genérico. Há `CopperNetIx` (estático) e
`DynamicNetIx` (por tick). Os índices são internos e podem ser densos:
`PinIx` indexa `CompiledCircuit.pins`, `CopperNetIx` indexa
`CompiledCircuit.copper_nets`, `SignalIx` indexa `SignalTable.slots` e
`DeviceIx` indexa `CompiledCircuit.devices`. O documento persistido
continua utilizando IDs estáveis.

### 8.4 Representação compilada

``` rust
pub struct CircuitFingerprint(pub u64);

pub struct CompiledCircuit {
    pub fingerprint: CircuitFingerprint,
    pub pins: Vec<CompiledPin>,
    pub copper_nets: Vec<CopperNet>,
    pub switches: Vec<CompiledSwitch>,
    pub sources: Vec<CompiledSource>,
    pub loads: Vec<CompiledLoad>,
    pub wires: Vec<CompiledWire>,
    pub junctions: Vec<CompiledJunction>,
    pub devices: Vec<CompiledDevice>,
    pub signals: SignalTable,
}

pub struct CompiledPin {
    pub pin: PinRef,
    pub net: CopperNetIx,
}

pub struct CopperNet {
    pub pins: Vec<PinIx>,
    pub wires: Vec<WireId>,
    pub junctions: Vec<JunctionId>,
}

pub struct CompiledWire {
    pub wire: WireId,
    pub net: CopperNetIx,
}

pub struct CompiledJunction {
    pub junction: JunctionId,
    pub net: CopperNetIx,
}

pub struct CompiledSwitch {
    pub symbol: SymbolId,
    pub a: CopperNetIx,
    pub b: CopperNetIx,
    pub control: SignalIx,
    pub normally_closed: bool,
}

pub struct CompiledSource {
    pub supply: SupplyId,
    pub conductor: Conductor,
    pub net: CopperNetIx,
    pub role: SourceRole,
}

pub enum SourceRole {
    Energy,
    Reference,
}

pub struct CompiledLoad {
    pub symbol: SymbolId,
    pub device: DeviceId,
    pub nets: Vec<CopperNetIx>,
    pub kind: LoadKind,
    pub supply_kind: SupplyKind,
    pub required: Vec<Conductor>,
    pub rating: LoadRating,
}

pub struct CompiledDevice {
    pub device: DeviceId,
    pub actuation: Option<SignalIx>,
    pub trip: Option<SignalIx>,
}
```

`CopperNet` é estática. `CompiledWire` e `CompiledJunction` existem para que
o snapshot consiga mapear IDs de documento para a net dinâmica sem expor
`CopperNetIx`. `CompiledSwitch` liga duas nets de cobre; o estado fechado é
calculado por tick como `if normally_closed { !signal } else { signal }`.
Um contato NF usa `normally_closed = true`; um NA usa `false`.

No M1, cada dispositivo tem no máximo uma carga associada. Essa invariante
permite que `DeviceState.energized` seja um único booleano. Se um modelo
futuro tiver múltiplas cargas por dispositivo, o snapshot precisa de uma
lista por carga em vez de um booleano no dispositivo.

O fingerprint é um hash estável e não criptográfico do circuito canônico
(por exemplo, FNV-1a sobre a representação ordenada). Ele serve para
recusar checkpoints incompatíveis, não como garantia de segurança. Se o
custo de colisão se tornar relevante, trocar por SHA-256 em uma etapa
posterior.

### 8.5 Tabela de sinais

`SignalTable` e `DeviceSignals` pertencem à topologia compilada, não à
camada de simulação. Assim, `electrical::solve` pode receber
`&DeviceSignals` sem depender de um tipo definido em `runtime`.

``` rust
pub struct SignalTable {
    pub slots: Vec<SignalSlot>,
}

pub struct SignalSlot {
    pub origin: SignalOrigin,
    pub kind: SignalKind,
    pub policy: SignalPolicy,
}

pub enum SignalOrigin {
    Input(InputId),
    Device(DeviceId),
    Timer(DeviceId),
    Thermal(DeviceId),
}

pub enum SignalKind {
    Actuation,
    TimerOutput,
    ThermalTrip,
    Input,
}

pub enum SignalPolicy {
    Level,
    RisingEdge,
    Latch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSignals {
    bits: Vec<bool>,
}

impl DeviceSignals {
    pub fn get(&self, ix: SignalIx) -> bool;
    pub fn set(&mut self, ix: SignalIx, value: bool);
    pub fn len(&self) -> usize;
}
```

Cada sinal tem uma origem estável, um tipo e uma política de atualização.
Botões, bobinas e saídas de temporizadores não compartilham índices por
acidente. A tabela é criada na compilação e é imutável durante a execução.

`DeviceSignals` é um vetor de bits indexado por `SignalIx`. `Vec<bool>` é
suficiente para o MVP; se o profiling mostrar custo, trocar por um bitset
sem alterar o contrato público.

### 8.6 Determinismo da compilação

Para que a ordem de componentes e fios não altere o resultado:

1.  Ordenar entidades por IDs estáveis antes de atribuir índices.
2.  Canonicalizar as listas de fontes, switches e cargas.
3.  Não expor como identidade pública a raiz arbitrária do Union-Find.
4.  Ordenar diagnósticos e listas de falhas.
5.  Comparar resultados normalizados nos testes.
6.  Calcular o fingerprint somente depois da canonicalização.

A garantia de determinismo bit a bit é apropriada para o motor lógico
inteiro/booleano. Um solver analógico futuro com ponto flutuante precisará
de critérios numéricos próprios.

------------------------------------------------------------------------

## 9. Solver elétrico discreto

### 9.1 Escopo

O solver do MVP trabalha com conectividade ideal e regras discretas. Ele
determina quais terminais pertencem à mesma rede condutiva, quais
condutores alcançam cada net dinâmica e se há incompatibilidades
configuradas.

Não calcula:

-   tensão instantânea;
-   corrente;
-   impedância;
-   queda de tensão;
-   divisão de tensão;
-   transitórios;
-   aquecimento físico.

### 9.2 Nets de cobre e nets dinâmicas

A conectividade de fios e junções é estática. O compilador cria uma
union-find sobre os pinos e une os endpoints de cada fio, através de suas
junções. O resultado são as **nets de cobre**.

A cada tick, o solver cria uma union-find sobre as nets de cobre (não sobre
os pinos) e une os pares de nets de cada switch fechado. O resultado são as
**nets dinâmicas**.

``` text
Compilação:
  pinos ──fios/junções──► net de cobre (estática)

Por tick:
  nets de cobre ──switches fechados──► net dinâmica
```

Isso reduz o custo por tick ao número de nets, não ao número de fios, e
elimina a ambiguidade da v0.2, que refazia o Union-Find dos fios a cada
tick.

### 9.3 Máscara de condutores e injeção de fontes

Cada terminal de fonte injeta um bit de `Conductor` na net de cobre à qual
pertence. Terminais de fonte de energia usam `SourceRole::Energy`; o
terminal de PE usa `SourceRole::Reference`.

Para cada net dinâmica:

``` text
conductors = OR dos bits de todas as fontes/referências no grupo
has_source = existe pelo menos uma fonte de energia no grupo
```

O bitmask é o conjunto de funções de condutor presentes, não um potencial.
Uma net com `L1|N` está em curto; uma net só com `N` é um retorno sem
energia; uma net só com `PE` é referência, não alimentação.

### 9.4 Curto-circuito e falta para PE

As combinações proibidas são definidas pelo perfil de alimentação, não por
"qualquer diferença de identificador".

  Perfil          Curto se a mesma net dinâmica contém
  --------------- -------------------------------------------------------
  DC              `DcPositive` e `DcNegative`
  Monofásico      `L1` e `Neutral`
  Trifásico       duas fases distintas quaisquer, ou uma fase e `Neutral`
  Qualquer        `ProtectiveEarth` e qualquer outro condutor

Além disso:

-   Fontes de perfis diferentes na mesma net dinâmica geram
    `OC-ELEC-006 incompatible-supplies`. O MVP não modela múltiplas fontes
    galvanicamente isoladas compartilhando uma referência; cada seção deve
    ter um perfil.
-   Uma net com `PE` e qualquer outro condutor gera
    `OC-ELEC-002 protective-earth-fault`, mesmo que a combinação não seja
    um curto de energia.
-   Uma net com uma fonte de energia, nenhum terminal de carga e nenhum
    terminal de switch gera `OC-ELEC-003 floating-net` como aviso; um ramo
    aberto é normal e não é erro fatal.

O curto é reportado contra IDs de documento. Como uma net dinâmica não tem
ID persistido, o sujeito primário é o terminal de menor ordenação
`(SymbolId, TerminalKey)` da net; os fios, junções e fontes envolvidos vão
em `related`, ordenados. Se a net não tiver terminal, usar o menor `WireId`.

### 9.5 Energização de cargas

Uma carga tem dois ou mais terminais. Cada terminal pertence a uma net
dinâmica. A carga é condutiva quando existe uma atribuição dos condutores
requeridos às nets dos terminais, com todas as nets distintas.

``` rust
pub enum LoadKind {
    Coil,
    Lamp,
    Buzzer,
    Motor,
}

pub enum MotorDirection {
    NotApplicable,
    Off,
    Forward,
    Reverse,
}

pub struct LoadState {
    pub load: LoadIx,
    pub conductive: bool,
    pub direction: MotorDirection,
}
```

Regras do MVP:

-   **Bobina, lâmpada, buzzer (dois terminais):** a carga é condutiva se
    uma net contém o primeiro condutor requerido e a outra contém o
    segundo, em qualquer ordem. Exemplo: `[L1, Neutral]` ou
    `[DcPositive, DcNegative]`.
-   **Motor trifásico (três terminais):** a carga é condutiva se as três
    nets contêm `L1`, `L2` e `L3`, na ordem dos terminais `U1`, `V1`, `W1`.
    A ordem de `CompiledLoad.nets` segue a ordem declarada pelo
    `LoadModel`; para o motor, `U1`, `V1`, `W1`. Se a ordem for
    `L1, L3, L2` em relação à sequência do perfil, a direção é `Reverse`;
    se for `L1, L2, L3`, é `Forward`; se faltar fase, é `Off`.
-   **Mesma net em dois terminais da carga:** a carga não é condutiva e
    gera `OC-ELEC-005 load-terminals-shorted`. O solver não trata a carga
    como curto de fonte; ela está apenas curto-circuitada por um fio ou
    contato.
-   **Condutor requerido ausente:** a carga não é condutiva; não é erro por
    si só, pois pode ser um contato aberto ou um ramo desligado.

A carga não é considerada energizada apenas porque uma de suas nets tem
fonte. É necessário o par (ou o conjunto trifásico) completo e compatível
com o tipo da carga.

### 9.6 Cargas em série e topologias não suportadas

O solver por conectividade ideal não resolve corretamente, de forma geral,
cargas em série, pontos estrela flutuantes ou divisões de tensão.

Não usar "potenciais virtuais" como solução genérica. Isso poderia
classificar redes inválidas como energizadas.

Regra explícita do M1:

> Uma net dinâmica sem fonte de energia e com dois ou mais terminais de
> carga é uma configuração não suportada. As cargas envolvidas não são
> energizadas e o runtime emite `OC-MOD-002 unsupported-series-load`.

Essa condição só aparece em runtime, quando um contato fecha e cria a net
sem fonte. A compilação não consegue prevê-la para todas as combinações de
switches. O diagnóstico é um aviso, não um erro fatal; a simulação
continua, mas as cargas afetadas ficam indeterminadas e marcadas como não
energizadas.

Estratégia de evolução:

-   MVP: declarar explicitamente os circuitos suportados e emitir
    diagnóstico para os não suportados.
-   Etapa intermediária: análise de caminhos e ramos, sem alegar cálculo
    analógico.
-   Etapa avançada: solver resistivo, por exemplo MNA, com modelos de
    componentes e critérios de convergência.

### 9.7 Compatibilidade de tensão e frequência

O solver não usa `nominal_voltage_mv` nem `frequency_millihz`. A
compatibilidade é uma checagem da camada lógica, depois que o solver
informa `conductive = true`:

-   A carga declara uma faixa `voltage_min_mv..voltage_max_mv` e, para AC,
    uma faixa de frequência.
-   O perfil de alimentação declara o valor nominal.
-   Se o nominal estiver fora da faixa, a carga não é acionada e o runtime
    emite `OC-ELEC-004 load-rating-mismatch`.
-   Para DC, a faixa de frequência deve ser `None`.
-   Para AC, o perfil precisa declarar `frequency_millihz` e a carga precisa
    declarar uma faixa; se faltar um dos dois, a carga não é acionada e o
    runtime emite `OC-ELEC-004`.

Isso mantém o solver independente de tensão e permite que o catálogo
modele bobinas de 24 V DC, 230 V AC e 400 V AC com faixas explícitas.

### 9.8 Contrato do solver

``` rust
pub struct ElectricalSolution {
    pub nets: Vec<DynamicNet>,
    pub copper_to_dynamic: Vec<DynamicNetIx>,
    pub loads: Vec<LoadState>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct DynamicNet {
    pub conductors: ConductorMask,
    pub has_source: bool,
    pub load_terminals: u16,
    pub switch_terminals: u16,
}

pub fn solve(
    circuit: &CompiledCircuit,
    signals: &DeviceSignals,
) -> ElectricalSolution;
```

O processo por tick:

1.  Criar union-find sobre `circuit.copper_nets`.
2.  Unir as nets de cada switch cujo estado efetivo é fechado.
3.  Calcular `conductors`, `has_source`, `load_terminals` e
    `switch_terminals` por net dinâmica.
4.  Validar combinações proibidas e emitir falhas.
5.  Avaliar `LoadState` para cada carga.
6.  Detectar nets sem fonte com duas ou mais cargas.
7.  Produzir `ElectricalSolution` com `copper_to_dynamic` para o snapshot.

O solver é uma função pura: não altera `DeviceSignals`, não conhece tempo e
não produz snapshot. A política de falha e o commit ficam no runtime.

------------------------------------------------------------------------

## 10. Motor lógico e semântica temporal

### 10.1 Decisão central: commit `current`/`next`

O motor não itera até ponto fixo. Ele mantém dois buffers de sinais:

-   `current`: sinais efetivos para a rede que será resolvida neste tick;
-   `next`: sinais calculados a partir da solução deste tick, efetivos no
    tick seguinte.

O ciclo é:

``` text
solve(current) → logic → next → commit(current = next)
```

Esse commit é a decisão central do motor. Ele quebra o laço algébrico
bobina → contato → bobina do circuito de selo e torna a simulação
determinística sem um solver de ponto fixo. A consequência é um atraso de
um tick na atuação de contatos controlados por lógica; isso é um atraso de
scan, não necessariamente o tempo mecânico real do contator.

Entradas externas são a exceção deliberada: elas são aplicadas na fronteira
do tick diretamente em `current`, porque são condições de contorno, não
saídas de lógica. Assim, pressionar S1 fecha o contato de S1 no mesmo tick;
a bobina K1 energiza nesse tick; o contato de selo de K1 fecha no tick
seguinte.

### 10.2 Ordem de execução

Cada tick `n` segue esta sequência:

1.  **Amostrar entradas:** receber `Inputs`, comparar com
    `inputs_previous` para detectar bordas de pulso e aplicar os sinais de
    entrada em `current`.
2.  **Resolver a rede:** chamar `electrical::solve` com `current`.
3.  **Avaliar a lógica:** copiar `current` para `next` e calcular bobinas,
    temporizadores, térmico, diagnósticos e demais sinais derivados a
    partir da solução.
4.  **Avaliar a política de falha:** se houver diagnóstico de runtime que
    trava, marcar `faulted`. O tick ainda completa e produz visão.
5.  **Reter a visão do tick:** guardar os sinais efetivos usados na
    solução, a solução elétrica, o estado efetivo de temporizadores e
    térmicos, os diagnósticos ativos e o flag de falha. Isso é uma visão
    compacta, não um `SimSnapshot` completo.
6.  **Registrar mudanças:** acrescentar ao change log as mudanças de sinais
    efetivos e de diagnósticos.
7.  **Confirmar o próximo estado:** `current = next`, e o mesmo para
    temporizadores e térmicos.
8.  **Avançar o tempo virtual:** `t_us += tick_duration_us`.

`SimSnapshot` é construído sob demanda a partir da última visão do tick.
Ele não mistura a solução com os sinais futuros: usa os sinais efetivos que
foram resolvidos e a solução correspondente. O estado `next` não é exposto
no snapshot.

### 10.3 Estado

``` rust
pub struct RuntimeState {
    pub tick: u64,
    pub t_us: u64,
    pub current: DeviceSignals,
    pub next: DeviceSignals,
    pub timers: Vec<TimerState>,
    pub next_timers: Vec<TimerState>,
    pub thermals: Vec<ThermalState>,
    pub next_thermals: Vec<ThermalState>,
    pub inputs_previous: Inputs,
    pub last_view: TickView,
    pub faulted: bool,
}

pub struct TickView {
    pub tick: u64,
    pub t_us: u64,
    pub solved: DeviceSignals,
    pub solution: ElectricalSolution,
    pub timers: Vec<TimerState>,
    pub thermals: Vec<ThermalState>,
    pub diagnostics: Vec<Diagnostic>,
    pub faulted: bool,
}
```

`timers` e `thermals` são indexados por `DeviceIx` e mapeados para
`DeviceId` por `CompiledDevice`. `current` e `next` têm o mesmo tamanho da
`SignalTable`. No início da fase de lógica, `next` é uma cópia de
`current`; sinais não atualizados mantêm o valor anterior. O mesmo padrão
vale para `next_timers` e `next_thermals`: a lógica calcula o próximo
estado, e o commit troca os buffers.

`TickView` é a memória de um único tick, não um histórico. Ela permite que
`snapshot()` seja chamado depois de `step()` sem reconstruir a solução nem
expor o estado `next`.

### 10.4 Interface do simulador

``` rust
pub enum TraceMode {
    Off,
    Changes,
}

pub struct Simulator {
    tick_duration_us: u32,
    circuit: CompiledCircuit,
    state: RuntimeState,
    trace_mode: TraceMode,
    trace: ChangeLog,
}

pub struct StepReport {
    pub ticks_completed: u32,
    pub faulted: bool,
}

pub enum SimulationError {
    InvalidTickDuration,
    AlreadyFaulted,
    UnknownInput(InputId),
    CircuitMismatch,
}

impl Simulator {
    pub fn new(
        circuit: CompiledCircuit,
        tick_duration_us: u32,
        trace_mode: TraceMode,
    ) -> Result<Self, SimulationError>;

    pub fn step(&mut self, inputs: &Inputs)
        -> Result<StepReport, SimulationError>;

    pub fn step_n(&mut self, ticks: u32, inputs: &Inputs)
        -> Result<StepReport, SimulationError>;

    pub fn snapshot(&self) -> SimSnapshot;

    pub fn tick(&self) -> u64;
    pub fn t_us(&self) -> u64;

    pub fn take_trace(&mut self) -> Option<ChangeLog>;
    pub fn reset_fault(&mut self);
}
```

Decisões:

-   `step` e `step_n` **não** alocam um `SimSnapshot` por tick. Eles
    atualizam a `TickView` compacta; `snapshot()` monta a estrutura
    completa sob demanda.
-   `snapshot()` devolve a visão do último tick concluído. Antes do
    primeiro `step`, devolve o estado inicial em `tick = 0`, `t_us = 0`,
    com a solução calculada para os sinais em repouso.
-   `step_n` aplica o mesmo conjunto de entradas por `n` ticks. Para mudar
    entradas, o chamador divide a execução em segmentos.
-   O tick que trava a simulação ainda conta como concluído. `step`
    devolve `Ok(StepReport { ticks_completed: 1, faulted: true })`;
    `step_n` para nesse tick e devolve o número de ticks concluídos.
    Chamadas seguintes devolvem `AlreadyFaulted` até `reset_fault()`.
-   O tick fixo pertence ao `Simulator`; o chamador não passa `dt`.
-   `take_trace` consome o log acumulado quando o modo é
    `TraceMode::Changes`; com `TraceMode::Off`, devolve `None`.
-   `CircuitMismatch` é usado ao restaurar um checkpoint com fingerprint ou
    configuração temporal diferente.
-   `Inputs` é validado contra a `SignalTable`; uma entrada não declarada
    gera `UnknownInput` e não é ignorada.

### 10.5 Entradas e bordas

`Inputs` representa o conjunto de entradas ativas no tick. A semântica vem
do catálogo:

-   `Momentary`: nível; o contato segue o estado ativo.
-   `Maintained`: nível; a UI mantém o estado alternado.
-   `Pulse`: borda de subida; a ação ocorre uma vez.

Uma entrada momentânea deve permanecer ativa por pelo menos um tick para
ser amostrada. A UI que roda vários ticks por frame deve manter o estado
ativo durante o lote de ticks do frame; um clique mais curto que um frame
pode ser perdido se a UI não o retiver. O CLI resolve isso declarando o
estado da entrada por tick.

Uma entrada não declarada pelo catálogo não é ignorada. O runtime devolve
`UnknownInput` antes de executar o tick, para que um cenário com nome errado
falhe de forma explícita.

### 10.6 Atraso de atuação

O atraso de um tick é uma consequência do modelo síncrono: contatos
controlados por uma bobina passam a ser efetivos no ciclo seguinte.

Isso não representa necessariamente o tempo mecânico real de um contator.
Em uma etapa posterior, introduzir parâmetros explícitos de pickup e
dropout:

``` rust
pub struct ActuatorTiming {
    pub pickup_delay_us: u64,
    pub dropout_delay_us: u64,
}
```

A oscilação de um circuito com contato NF realimentando a própria bobina
deve ser determinada por estados e tempos de atuação definidos, não pelo
frame rate da GUI.

### 10.7 Política pós-falha

Falhas elétricas não são erros Rust; são resultados da simulação. A
política padrão do M1 é **falha fechada com latch**:

1.  O tick que detecta um curto, falta para PE ou fornecimento
    incompatível completa normalmente.
2.  O snapshot desse tick contém o diagnóstico e `faulted = true`.
3.  O runtime entra em estado `faulted`; chamadas seguintes a `step`
    retornam `SimulationError::AlreadyFaulted`.
4.  `reset_fault()` limpa o latch, mas o circuito compilado não muda. Na
    prática, o chamador deve corrigir o documento, recompilar e criar um
    novo simulador. O reset existe para que a UI possa rearmar a execução
    sem recriar o objeto.

Diagnósticos que **não** travam a simulação:

-   `OC-ELEC-004 load-rating-mismatch` (Error): a carga não é acionada,
    mas o restante do circuito continua.
-   `OC-ELEC-005 load-terminals-shorted` (Error): a carga não é energizada,
    mas o circuito continua.
-   `OC-MOD-002 unsupported-series-load` (Warning): as cargas envolvidas
    não são energizadas.

Diagnósticos que travam:

-   `OC-ELEC-001 short-circuit`
-   `OC-ELEC-002 protective-earth-fault`
-   `OC-ELEC-006 incompatible-supplies`

Justificativa: continuar após um curto produziria estados elétricos
fictícios e poderia mascarar o erro. A falha fechada também torna o golden
do reversor determinístico: o caso simultâneo termina no primeiro curto,
não em uma oscilação indefinida.

O CLI retorna código 2 para falha elétrica travada; um cenário pode
declarar `expect_fault` para tratar isso como resultado esperado.

### 10.8 Trace e change log

O trace do golden é um log de mudanças, não um snapshot por tick. Em 10 s a
1 ms, um snapshot por tick seriam 10 mil estruturas; um log registra apenas
o que mudou.

``` rust
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeLog {
    pub tick_duration_us: u32,
    pub changes: Vec<SignalChange>,
    pub diagnostics: Vec<DiagnosticChange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalChange {
    pub tick: u64,
    pub signal: SignalRef,
    pub before: bool,
    pub after: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalRef {
    Input(InputId),
    DeviceEnergized(DeviceId),
    SymbolClosed(SymbolId),
    TimerOutput(DeviceId),
    ThermalTripped(DeviceId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticChange {
    pub tick: u64,
    pub action: DiagnosticAction,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticAction {
    Raised,
    Cleared,
}
```

Regras:

-   O tick de uma mudança é o tick em que o novo valor é efetivo ou
    observado.
-   Entradas são registradas no tick em que são aplicadas.
-   `DeviceEnergized` é a energização da carga resolvida no tick.
-   `SymbolClosed`, `TimerOutput` e `ThermalTripped` são registrados no
    tick em que o sinal confirmado se torna efetivo.
-   O progresso de `elapsed_us` de um temporizador não é registrado a cada
    tick; o snapshot sob demanda mostra o valor. O change log registra
    apenas a mudança de saída.
-   Diagnósticos são registrados quando aparecem e quando desaparecem, não
    a cada tick.

O formato JSON do trace é definido em `SIMULATION.md` e usado pelos goldens.

------------------------------------------------------------------------

## 11. Temporizadores e dispositivos

### 11.1 TON

-   Entrada desligada: saída desligada e temporização reiniciada.
-   Entrada ligada: inicia a contagem.
-   Preset atingido: saída ligada no tick seguinte ao commit; se a entrada
    cair antes do commit, a conclusão é cancelada.

### 11.2 TOF

-   Entrada ligada: saída liga (após o commit de um tick).
-   Entrada desligada: inicia a temporização de desligamento.
-   Durante o preset: saída permanece ligada.
-   Preset atingido: saída desliga no tick seguinte ao commit.
-   Nova entrada ligada: cancela a temporização de desligamento.

### 11.3 Limite do preset

O temporizador acumula `tick_duration_us` por tick. A comparação é
`elapsed_next >= preset_us`. O preset é arredondado para cima em ticks: um
preset que não é múltiplo do tick completa no primeiro tick em que o
acumulado alcança ou ultrapassa o valor.

A regra de fronteira é:

> A entrada do tick é processada antes da comparação. Uma mudança de
> entrada no tick em que o preset seria atingido pode cancelar a
> conclusão. Depois que a saída é confirmada no commit, uma mudança de
> entrada no tick seguinte não cancela retroativamente a saída; ela é
> processada na lógica desse tick e vale para o tick posterior.

Exemplo com TON, `tick = 1 ms`, `preset = 3 ms` (3 ticks), entrada ativa
desde o tick 0:

  Tick   Entrada   `elapsed` atual   `elapsed` próximo   Saída atual   Saída próxima
  ------ ---------- ----------------- ------------------- ------------- ---------------
  0      1          0                 1                   0             0
  1      1          1                 2                   0             0
  2      1          2                 3 (>= preset)       0             1
  3      1          3                 3                   1             1

A saída se torna efetiva no tick 3. Se a entrada cair no tick 2, o
`elapsed` próximo volta a 0 e a saída nunca completa. Se a entrada cair no
tick 3, a saída já está efetiva nesse tick e desliga no tick 4.

Exemplo com TOF, mesmo preset, considerando a saída já ligada e a entrada
desligada no tick 0:

  Tick   Entrada   `elapsed` atual   `elapsed` próximo   Saída atual   Saída próxima
  ------ ---------- ----------------- ------------------- ------------- ---------------
  0      0          0                 1                   1             1
  1      0          1                 2                   1             1
  2      0          2                 3 (>= preset)       1             0
  3      0          3                 3                   0             0

A saída desliga no tick 3. Se a entrada voltar a 1 no tick 2, o
`elapsed` próximo volta a 0 e a saída permanece ligada. Se voltar no tick
3, a saída já está desligada e liga no tick 4.

### 11.4 Estado do temporizador

``` rust
pub enum TimerKind {
    Ton,
    Tof,
}

pub enum TimerPhase {
    Idle,
    Timing,
    Active,
    Retiming,
}

pub struct TimerState {
    pub phase: TimerPhase,
    pub elapsed_us: u64,
    pub output: bool,
    pub previous_input: bool,
}
```

`TimerState` no snapshot representa o estado efetivo do tick retido em
`TickView.timers`, não o `next`. O modelo pode ser simplificado se os testes
demonstrarem que menos fases bastam; a semântica observável é mais
importante que a quantidade de variantes.

### 11.5 Relé térmico: latch e reset

``` rust
pub struct ThermalState {
    pub tripped: bool,
}
```

O térmico do M1 é um latch, não um modelo térmico. Ele não mede corrente,
temperatura nem tempo de aquecimento.

Entradas:

-   `trip`: pulso de teste manual. Na borda de subida, `tripped = true`.
-   `reset`: pulso manual. Na borda de subida, se `tripped`, `tripped =
    false`.

Regras:

-   `trip` tem precedência sobre `reset` no mesmo tick; o latch permanece.
-   Enquanto `tripped`, o contato NF do térmico permanece aberto.
-   O reset não reenergiza o contator. Depois do reset, o operador precisa
    pressionar LIGA novamente.
-   O contato NF controlado pelo térmico usa o estado `current`; portanto,
    uma mudança de `tripped` no tick `n` afeta o contato no tick `n+1`.
-   Um parâmetro futuro `reset_inhibit_us` pode modelar o tempo de
    resfriamento; não faz parte do M1.

### 11.6 Matriz mínima de comportamento

  Dispositivo    Entrada/condição                      Resultado
  -------------- ------------------------------------- -------------------------------------------
  Botão NA       Pressionado                           Contato fechado no mesmo tick
  Botão NF       Pressionado                           Contato aberto no mesmo tick
  Bobina         Energizada                            Sinal de atuação do dispositivo ativo no tick seguinte
  Contator       Bobina ativa                          Contatos associados atuam no tick seguinte
  Relé térmico   `trip` na borda de subida             Latch ativo; NF abre no tick seguinte
  Relé térmico   `reset` na borda de subida             Latch limpo; NF fecha no tick seguinte
  TON            Preset atingido                       Saída ativa no tick seguinte ao commit
  TOF            Entrada desligada e preset expirado   Saída desativa no tick seguinte ao commit
  Lâmpada        Alimentação compatível                Indicador ativo no mesmo tick
  Motor          Três fases presentes                  Gira (direção conforme sequência); sem torque/corrente

Cada comportamento deve possuir testes automatizados.

### 11.7 Partida direta de referência

O cenário de referência do M1 inclui:

-   **Q1:** interruptor com retenção, polo de comando e polos principais.
    Um único `InputKey` `closed` controla todos os símbolos.
-   **F1:** relé térmico com contato NF no comando e entradas `trip` e
    `reset`. Os elementos de aquecimento do circuito de potência não são
    modelados; a proteção é representada pelo contato NF e pelo latch.
-   **S1:** botão NA momentâneo de LIGA.
-   **S2:** botão NF momentâneo de DESLIGA.
-   **K1:** contator com bobina, contato de selo NA e contatos principais.
-   **M1:** motor trifásico simplificado de três terminais.

Circuito de comando:

``` text
Q1 (closed) ─ F1 (NC) ─ S2 (NC) ─┬─ S1 (NO) ─┬─ K1 coil ─ N
                                 └─ K1 13/14 ┘
```

Circuito de potência:

``` text
Q1 (3P) ─ K1 (3P) ─ F1 (aquecimento não modelado) ─ M1
```

O motor é energizado quando L1, L2 e L3 alcançam U1, V1 e W1. A direção é
determinada pela ordem das fases. O ponto estrela e os enrolamentos de seis
terminais ficam para M3.

------------------------------------------------------------------------

## 12. Snapshot e integração com a interface

O snapshot é uma estrutura imutável que representa um único instante
lógico: a rede que foi efetivamente resolvida no último tick concluído. Ele
é construído sob demanda a partir da `TickView`, usa somente IDs de
documento e tipos de `circuit-model`, e não expõe `CopperNetIx`,
`DynamicNetIx` ou qualquer índice interno.

``` rust
pub struct SimSnapshot {
    pub tick: u64,
    pub t_us: u64,
    pub wires: Vec<WireState>,
    pub junctions: Vec<JunctionState>,
    pub symbols: Vec<SymbolState>,
    pub devices: Vec<DeviceState>,
    pub timers: Vec<TimerSnapshot>,
    pub diagnostics: Vec<Diagnostic>,
    pub faulted: bool,
}

pub struct WireState {
    pub wire: WireId,
    pub conductors: ConductorMask,
    pub energized: bool,
}

pub struct JunctionState {
    pub junction: JunctionId,
    pub conductors: ConductorMask,
    pub energized: bool,
}

pub struct SymbolState {
    pub symbol: SymbolId,
    pub closed: Option<bool>,
}

pub struct DeviceState {
    pub device: DeviceId,
    pub energized: bool,
    pub tripped: bool,
    pub direction: Option<MotorDirection>,
}

pub struct TimerSnapshot {
    pub timer: DeviceId,
    pub phase: TimerPhase,
    pub elapsed_us: u64,
    pub output: bool,
}
```

Semântica:

-   `WireState.conductors` é a máscara da net dinâmica à qual o fio
    pertence. `energized` indica que a net tem pelo menos uma fonte de
    energia; não significa "seguro para tocar".
-   `SymbolState.closed` é `Some(true/false)` para contatos e `None` para
    bobinas, lâmpadas, motores e terminais de fonte. O valor vem de
    `last_view.solved`, ou seja, é o estado que foi efetivamente usado na
    solução.
-   `DeviceState.energized` é a energização da carga resolvida em
    `last_view.solution`. Para um contator, pode ser `true` enquanto o
    contato ainda está aberto; o contato fecha no tick seguinte. Essa
    diferença é intencional e representa o atraso de scan.
-   `DeviceState.tripped` vem de `last_view.thermals`. `direction` só é
    preenchido para motores.
-   `TimerSnapshot` vem de `last_view.timers`, o estado efetivo do tick,
    não o `next`.
-   `diagnostics` contém os diagnósticos de runtime ativos em
    `last_view.diagnostics`, ordenados por código, sujeito e relacionados.
    Diagnósticos de compilação são devolvidos por `compile`, não pelo
    snapshot.
-   `faulted` reflete `last_view.faulted` e indica que o runtime entrou em
    latch de falha.

Todos os vetores são ordenados por ID (`WireId`, `JunctionId`, `SymbolId`,
`DeviceId`) para serialização e comparação determinísticas. Evitar
`HashMap` na saída.

O tipo `SimSnapshot` reside em `circuit-model::snapshot`. O renderer e o
viewer dependem de `circuit-model`, não de `circuit-sim`.

### 12.1 Snapshot versus change log

O snapshot responde "como está o circuito neste instante?". O change log
responde "o que mudou entre tiques?". São contratos diferentes:

-   A GUI usa `snapshot()` sob demanda, uma vez por frame ou por
    inspeção.
-   O CLI e os goldens usam `ChangeLog` para comparar traces sem armazenar
    um snapshot por tick.
-   Um teste que precisa de um instante específico chama `step_n` até o
    tick desejado e então `snapshot()`.

------------------------------------------------------------------------

## 13. Diagnósticos

### 13.1 Categorias

  Categoria               Exemplos                              Momento
  ----------------------- ------------------------------------- ----------------------
  Estrutural              ID duplicado, terminal inexistente,   Edição/compilação
                          junção sem conexão

  Semântica               Modelo inexistente, versão            Compilação
                          incompatível, parâmetro fora da faixa

  Elétrica                Curto, falta para PE, carga           Execução
                          curto-circuitada, tensão de bobina
                          incompatível

  Limitação do modelo     Topologia não suportada, carga em     Compilação/execução
                          série
  ----------------------- ------------------------------------- ----------------------

### 13.2 Códigos string

`DiagnosticCode` é um código string estável e extensível. Não é um enum
fechado; novos códigos podem ser adicionados sem quebrar consumidores que
já tratam os códigos existentes.

``` rust
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCode(&'static str);

impl DiagnosticCode {
    pub const DUPLICATE_DEVICE_REFERENCE: Self = Self("OC-STR-001");
    pub const DUPLICATE_ID: Self = Self("OC-STR-002");
    pub const MISSING_REQUIRED_TERMINAL: Self = Self("OC-STR-003");
    pub const INVALID_CONNECTION: Self = Self("OC-STR-004");
    pub const DANGLING_JUNCTION: Self = Self("OC-STR-005");
    pub const INVALID_NEXT_ID: Self = Self("OC-STR-006");

    pub const UNKNOWN_DEVICE_REFERENCE: Self = Self("OC-SEM-001");
    pub const UNKNOWN_MODEL: Self = Self("OC-SEM-002");
    pub const MODEL_VERSION_MISMATCH: Self = Self("OC-SEM-003");
    pub const UNKNOWN_SYMBOL_MODEL: Self = Self("OC-SEM-004");
    pub const UNKNOWN_TERMINAL: Self = Self("OC-SEM-005");
    pub const PARAMETER_OUT_OF_RANGE: Self = Self("OC-SEM-006");

    pub const SHORT_CIRCUIT: Self = Self("OC-ELEC-001");
    pub const PROTECTIVE_EARTH_FAULT: Self = Self("OC-ELEC-002");
    pub const FLOATING_NET: Self = Self("OC-ELEC-003");
    pub const LOAD_RATING_MISMATCH: Self = Self("OC-ELEC-004");
    pub const LOAD_TERMINALS_SHORTED: Self = Self("OC-ELEC-005");
    pub const INCOMPATIBLE_SUPPLIES: Self = Self("OC-ELEC-006");

    pub const UNSUPPORTED_TOPOLOGY: Self = Self("OC-MOD-001");
    pub const UNSUPPORTED_SERIES_LOAD: Self = Self("OC-MOD-002");

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}
```

O JSON e o trace usam a string (`"OC-ELEC-001"`), não o nome do variant
Rust. A lógica não depende da mensagem localizável.

### 13.3 Severidade e latch

``` rust
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl DiagnosticCode {
    pub fn severity(self) -> Severity;
    pub fn latches_simulation(self) -> bool;
}
```

  Código         Severidade   Trava a simulação?
  -------------- ------------ -------------------
  OC-STR-*       Error        Não (falha de compilação)
  OC-SEM-*       Error        Não (falha de compilação)
  OC-ELEC-001    Error        Sim
  OC-ELEC-002    Error        Sim
  OC-ELEC-003    Warning      Não
  OC-ELEC-004    Error        Não
  OC-ELEC-005    Error        Não
  OC-ELEC-006    Error        Sim
  OC-MOD-001     Error        Não (falha de compilação)
  OC-MOD-002     Warning      Não

`latches_simulation` só é consultado para diagnósticos de runtime. Uma
falha de compilação impede a criação do `CompiledCircuit` e, portanto, não
chega ao runtime.

### 13.4 Estrutura

``` rust
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Subject {
    Document,
    Device(DeviceId),
    Symbol(SymbolId),
    Terminal(PinRef),
    Wire(WireId),
    Junction(JunctionId),
    Supply(SupplyId),
    Input(InputId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub subject: Subject,
    pub related: Vec<Subject>,
    pub message: String,
}
```

`PinRef` e `InputId` também precisam de `Ord` para que `Subject` seja
ordenável. A ordenação é por variante, depois pelo ID interno; isso é
suficiente para serialização e deduplicação determinísticas.

`related` é ordenado e sem duplicatas. Para uma falha de net, o sujeito
primário é um terminal ou fio da net; os demais elementos vão em
`related`. A identidade de um diagnóstico para deduplicação é
`(code, subject, related)`, sem a mensagem.

### 13.5 Diagnósticos ativos

O runtime mantém um conjunto ordenado de diagnósticos ativos. A cada tick:

1.  Coletar os diagnósticos elétricos e de limitação do solver. Os
    diagnósticos de compilação não entram aqui: eles pertencem ao
    `CompileOutput`.
2.  Comparar com o conjunto ativo por identidade.
3.  Registrar `Raised` para os novos e `Cleared` para os que sumiram.
4.  Guardar o conjunto ativo para o snapshot.

Um diagnóstico que persiste por vários ticks aparece uma vez no snapshot,
não uma vez por tick no change log.

### 13.6 Terminais desconectados

Nem todo terminal desconectado é erro. A obrigatoriedade é definida no
catálogo (`TerminalDefinition.required`).

Exemplos:

-   Contato auxiliar não utilizado: normalmente permitido.
-   Bobina sem circuito: aviso ou erro conforme contexto.
-   Motor sem alimentação: aviso.
-   Terminal obrigatório ausente: erro.
-   Fonte sem carga: informação ou aviso.
-   Junção sem conexão: erro estrutural (`OC-STR-005`).
-   Terminal de fonte sem fio: aviso, não erro fatal.

Um terminal desconectado não é automaticamente um curto nem uma carga em
série. A distinção é feita pela topologia e pelo estado dos switches.

------------------------------------------------------------------------

## 14. Editor gráfico e renderização

### 14.1 Viewer antes do editor

A GUI começa com um viewer egui somente leitura, em M2a. O objetivo é
validar o contrato `Schematic + SimSnapshot → renderer` antes de investir
em comandos, undo/redo e roteamento.

O viewer:

-   carrega um `.ocircuit` via `circuit-project`;
-   compila e simula via `circuit-sim`;
-   desenha fios e junções coloridos pela máscara de condutores;
-   destaca contatos fechados, bobinas energizadas, motor e térmico;
-   mostra o painel de diagnósticos;
-   oferece play, pause, step, reset e velocidade de simulação;
-   não altera o documento.

O editor completo (M2b) adiciona `circuit-commands`, `circuit-editor` e
`circuit-renderer` com seleção, movimentação, rotação, exclusão, fios,
junções e undo/redo.

### 14.2 UI

A aplicação desktop utiliza egui/eframe. O código de UI fica
exclusivamente em `apps/desktop`.

Regiões sugeridas:

-   Toolbar.
-   Biblioteca de componentes.
-   Canvas.
-   Inspector de propriedades.
-   Painel inferior de diagnósticos e eventos.
-   Controles de simulação (play, pause, step, reset, velocidade).

### 14.3 Coordenadas

O documento usa unidades lógicas inteiras. O renderer converte para
coordenadas de tela por meio de uma câmera:

``` rust
pub struct Camera {
    pub offset: Vec2,
    pub zoom: f32,
}
```

`Vec2` aqui é um tipo neutro do renderer, não `egui::Vec2`. Devem existir
transformações inversas para hit-test e edição.

### 14.4 Cena neutra

``` rust
pub fn build_scene(
    schematic: &Schematic,
    view: &View,
    snapshot: Option<&SimSnapshot>,
) -> Scene;
```

A cena contém primitivas e metadados suficientes para desenho, seleção,
exportação e testes. Não depende de egui. O renderer não acessa
`CompiledCircuit` nem índices internos; toda a informação vem do documento
e do snapshot.

### 14.5 Desempenho

Criar benchmarks quando houver um viewer funcional, com documentos de 100,
500 e 2.000 componentes, medindo:

-   tempo de renderização;
-   hit-test;
-   roteamento;
-   compilação da topologia;
-   simulação;
-   invalidação e reconstrução de cache.

Usar culling por bounding box e invalidação seletiva. Não introduzir
otimizações complexas sem medição. A simulação a 1 ms e 60 fps executa
cerca de 16 ticks por frame; o viewer deve chamar `step_n(16, ...)` e um
único `snapshot()` por frame, não 16 snapshots.

### 14.6 Undo e redo

Undo/redo é M2b, não M1. A estratégia é Command Pattern com histórico
transacional:

``` rust
pub trait Command {
    fn apply(&mut self, schematic: &mut Schematic)
        -> Result<(), CommandError>;

    fn revert(&mut self, schematic: &mut Schematic)
        -> Result<(), CommandError>;
}
```

Uma operação inválida não pode deixar o documento parcialmente alterado. O
comando deve validar antes de mutar ou trabalhar sobre uma cópia e
confirmar no final.

------------------------------------------------------------------------

## 15. Persistência e formato `.ocircuit`

### 15.1 Formato

O formato inicial é JSON, com extensão `.ocircuit`.

Requisitos:

-   `schema_version` obrigatório;
-   `next_id` obrigatório;
-   IDs estáveis e únicos;
-   ordem de serialização determinística;
-   validação antes de retornar o documento;
-   migrações explícitas;
-   salvamento atômico na camada de aplicação;
-   nenhuma promessa de preservação de campos desconhecidos;
-   round-trip garantido somente dentro da mesma versão.

Exemplo ilustrativo (schema 1):

``` json
{
  "schema_version": 1,
  "next_id": 42,
  "name": "Partida Direta",
  "grid_unit": 10,
  "devices": [
    {
      "id": 1,
      "reference": "K1",
      "model_id": "contactor.3p",
      "model_version": 1,
      "parameters": {}
    }
  ],
  "symbols": [
    {
      "id": 10,
      "device_id": 1,
      "symbol_model_id": "contactor.3p.coil",
      "position": { "x": 0, "y": 0 },
      "rotation": "deg0"
    }
  ],
  "junctions": [],
  "wires": [],
  "supplies": []
}
```

O exemplo é mínimo; um projeto real contém fontes, fios, junções e os
demais símbolos. Os arrays são serializados ordenados por ID; a ordem de
`Wire.route` é preservada porque os waypoints são significativos.

### 15.2 Versionamento

  Versão encontrada   Comportamento
  ------------------- -------------------------------------------------------
  `== CURRENT`        Desserializar, validar e retornar.
  `< CURRENT`         Aplicar migrações em ordem e validar o resultado.
  `> CURRENT`         Recusar com `UnsupportedSchemaVersion { found, supported }`.

Regras:

-   v1 recusa `schema_version` maior com erro claro. Não tenta "ler o que
    der".
-   v1 usa `#[serde(deny_unknown_fields)]`. Um campo desconhecido é erro,
    não é ignorado silenciosamente.
-   Não há promessa de preservar campos desconhecidos. Qualquer adição de
    campo incrementa `schema_version` e ganha migração.
-   Round-trip garantido:
    `canonicalize(load(save(doc))) == canonicalize(doc)` para documentos
    v1 válidos.
-   A validação estrutural e de `next_id` ocorre antes de retornar o
    documento.

### 15.3 Responsabilidade de I/O

`circuit-project` converte estruturas em JSON e vice-versa, mas não acessa
diretamente `std::fs`.

A aplicação desktop ou CLI é responsável por abrir, gravar, escolher
caminhos e realizar salvamento atômico:

1.  escrever em um arquivo temporário no mesmo diretório;
2.  flush e `fsync`;
3.  renomear sobre o destino;
4.  remover o temporário em caso de erro.

O `circuit-project` devolve `Vec<u8>` ou um erro tipado; não conhece
paths.

### 15.4 Migrações

Cada versão de schema deve ter migração testada. Não alterar silenciosamente
a semântica de terminais ou componentes. A versão do modelo do componente
pode ser distinta da versão geral do documento.

Uma migração:

-   recebe o JSON da versão anterior;
-   produz o JSON da versão seguinte;
-   é idempotente na versão de destino;
-   tem teste de documento real, não apenas de campo isolado;
-   nunca inventa IDs; usa `next_id` ou calcula o próximo livre.

---

## 16. CLI headless

O crate `apps/cli` permite executar e validar circuitos sem abrir a
interface gráfica.

Comandos propostos:

``` text
opencircuit validate projeto.ocircuit
opencircuit simulate projeto.ocircuit --scenario partida-direta.json
opencircuit simulate projeto.ocircuit --scenario reversor.json --trace trace.json
opencircuit test examples/
opencircuit export projeto.ocircuit --format svg   # M3
```

Comportamento:

-   `validate`: carrega, valida e compila; imprime diagnósticos legíveis;
    retorna 0 se não houver erro.
-   `simulate`: carrega o cenário, executa os ticks, imprime o change log
    e os diagnósticos; `--json` produz saída estruturada.
-   `test`: percorre os cenários de `examples/` e compara o trace com o
    golden; retorna 1 se algum falhar.
-   `export`: só existe quando `circuit-export` entrar em M3.

Códigos de saída:

  Código   Significado
  -------- ---------------------------------------------------------------
  0        Sucesso; nenhum diagnóstico de erro.
  1        Erro de arquivo, schema, validação ou compilação.
  2        Falha elétrica travada durante a simulação.
  3        Erro de uso da CLI ou I/O.

A CLI é importante para testes de regressão, automação e uso em pipelines
de documentação. Ela não depende de egui.

------------------------------------------------------------------------

## 17. Testes e qualidade

### 17.1 Testes unitários

-   alocação e validação de `next_id`;
-   resolução de modelos, versões e terminais;
-   Union-Find de nets de cobre;
-   máscara de condutores e regras de curto por perfil;
-   contatos NA/NF e intertravamento;
-   bobinas e compatibilidade de tensão/frequência;
-   TON e TOF, incluindo fronteira do preset;
-   latch e reset do térmico;
-   política pós-falha;
-   serialização e migrações;
-   comandos undo/redo (M2b);
-   transformações de coordenadas (M2).

### 17.2 Golden tests

Cada exemplo contém:

-   documento `.ocircuit` ou, enquanto `circuit-project` não existir, uma
    fixture equivalente construída em memória;
-   cenário de entradas por tick;
-   configuração temporal (`tick_duration_us`);
-   trace esperado em change log;
-   diagnósticos esperados;
-   expectativa de falha, quando aplicável.

Formato conceitual do trace:

``` json
{
  "tick_duration_us": 1000,
  "ticks": 12,
  "faulted": false,
  "changes": [
    {
      "tick": 0,
      "signal": { "kind": "input", "device": 4, "key": "press" },
      "before": false,
      "after": true
    },
    {
      "tick": 0,
      "signal": { "kind": "device_energized", "device": 1 },
      "before": false,
      "after": true
    },
    {
      "tick": 1,
      "signal": { "kind": "symbol_closed", "symbol": 12 },
      "before": false,
      "after": true
    }
  ],
  "diagnostics": []
}
```

O trace não contém um snapshot por tick. Se um teste precisar de um
instante específico, ele chama `step_n` até o tick e então `snapshot()`.

Cenários:

**Partida direta** — Q1 fechado; F1 resetado; S2 NF fechado; S1 NA
pressionado; K1 e contato de selo; motor como carga trifásica
simplificada; F1 com `trip` e `reset`. O teste cobre:

-   S1 pressionado → K1 energiza;
-   S1 solto → K1 mantém pelo selo;
-   S2 pressionado → K1 desenergiza;
-   F1 `trip` → K1 cai; F1 `reset` não reenergiza; S1 reenergiza;
-   Q1 aberto → comando sem alimentação.

**Reversor** — K1 e K2; intertravamento elétrico por NF do outro contator;
sequência de fases invertida no motor; verificação de que não há
acionamento simultâneo proibido no caso realista.

O caso simultâneo de S1 e S2 também é golden:

-   no tick `n`, as duas bobinas energizam, porque os NF de intertravamento
    ainda estão fechados;
-   no tick `n+1`, os dois NF abrem e as duas bobinas caem; os contatos
    principais de K1 e K2 estão fechados nesse tick, gerando
    `OC-ELEC-001 short-circuit`;
-   com a política de falha fechada, a simulação trava em `n+1` e o golden
    verifica `faulted = true`.
-   A oscilação de período 2 é uma análise do modelo sem proteção; o
    golden registra o primeiro curto, não uma execução indefinida.

**Estrela-triângulo** — KM1 principal; KM2 triângulo; KM3 estrela; KT1
temporizador; teste de ausência de sobreposição entre estrela e triângulo.
Este cenário é uma referência de requisitos em M1; sua execução funcional
depende do modelo de enrolamentos e do solver adequado em M3.

### 17.3 Testes de propriedades

Com `proptest`:

-   permutação de componentes e fios não altera o circuito canônico nem o
    trace normalizado;
-   recompilar o mesmo documento gera o mesmo fingerprint;
-   salvar e carregar preserva a semântica dentro da mesma versão;
-   clone/checkpoint/restore e replay reproduzem o trace;
-   operações de conexão e desconexão não deixam referências inválidas;
-   TON nunca liga antes do preset; TOF nunca desliga antes do preset;
-   uma vez em latch de falha, nenhum tick adicional é executado.

### 17.4 Replay e rewind

`Clone` permite copiar um estado, mas não implementa rewind sozinho.

A estratégia combina:

``` text
Input Event Log
      │
      ▼
Periodic Checkpoints
      │
      ▼
State Restore
      │
      ▼
Deterministic Replay
```

Cada checkpoint é associado ao `CircuitFingerprint`, à configuração
temporal e ao tick. Não restaurar um estado em uma versão incompatível do
circuito. O replay aplica os mesmos eventos de entrada e compara o change
log.

### 17.5 Viewer e UI

Em M2a, testar o viewer com:

-   abrir o fixture de partida direta;
-   avançar ticks e verificar que K1 e o motor mudam;
-   abrir o painel de diagnósticos;
-   verificar que o snapshot não muda quando a simulação está pausada;
-   verificar que o renderer não depende de `circuit-sim`.

Em M2b, usar `egui_kittest` para fluxos essenciais:

-   selecionar componente;
-   mover, rotacionar e excluir;
-   conectar terminais;
-   desfazer e refazer;
-   salvar e reabrir.

### 17.6 CI

Matriz inicial:

-   Linux;
-   Windows;
-   macOS.

Verificações:

``` text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo deny check
cargo run -p xtask -- check-architecture
cargo check -p circuit-sim --target wasm32-unknown-unknown
```

Quando a aplicação web estiver em desenvolvimento, adicionar build e
testes do alvo WASM. Não assumir que a compilação do motor garante a
portabilidade da aplicação inteira.

------------------------------------------------------------------------

## 18. Exportação e símbolos

### 18.1 Exportação

O exportador recebe `Scene`, não estruturas internas de egui.

Formatos:

-   SVG é o primeiro formato, em M3; é vetorial, testável e não exige
    dependência pesada.
-   PDF é um spike separado. O CADCraft usa `krilla` e patches de
    `hayro-syntax`/`hayro-jbig2`; isso é uma pista, não uma decisão.
-   PNG ou outros formatos vetoriais podem entrar depois, se houver
    necessidade.

O spike de PDF deve comparar, no mínimo:

-   licença compatível com MIT/Apache-2.0;
-   suporte a texto, vetores e fontes incorporadas;
-   qualidade de saída para símbolos e tabelas;
-   manutenção e versão;
-   portabilidade para WASM, se o exportador web for requisito.

Não bloquear M1/M2 por causa do PDF. A exportação é executável pela CLI
quando `circuit-export` existir.

### 18.2 Símbolos e normas

O projeto criará símbolos próprios seguindo convenções eletrotécnicas
documentadas. Não copiar arquivos gráficos de bibliotecas externas sem
verificar licença e autorização.

A IEC 60617 é uma referência importante para símbolos gráficos
eletrotécnicos. O uso de símbolos não deve ser descrito como conformidade
normativa certificada sem revisão técnica e jurídica apropriada. Também
devem ser consideradas as normas ABNT/NBR aplicáveis ao público e às
jurisdições atendidas.

Cada símbolo deve ter metadados de origem, revisão e referência normativa,
sem incorporar conteúdo protegido sem licença.

------------------------------------------------------------------------

## 19. Segurança, licenças e governança

### 19.1 Licença

Licença pretendida: dual MIT OR Apache-2.0.

Manter os arquivos de licença na raiz e incluir política para contribuições
e dependências. Não incorporar código GPL ou outras licenças incompatíveis
sem decisão explícita sobre a licença e distribuição do projeto. A
compatibilidade deve ser analisada dependência por dependência.

### 19.2 Governança

Arquivos recomendados:

``` text
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

-   toda funcionalidade do motor deve ter testes;
-   todo componente deve declarar seus terminais e sua proveniência;
-   mudanças de schema exigem migração;
-   mudanças de comportamento exigem regressão;
-   dependências Git exigem revisão e responsável;
-   recursos gráficos devem ter proveniência e licença registradas;
-   o CI verifica formatação, lints, testes, licenças, fontes e camadas.

### 19.3 Documentação e ADRs

Quatro documentos planejados mais este aumentam o risco de drift. A regra
é: a partir de M0, os tipos reais no código são a fonte de verdade; os
documentos registram decisões e invariantes, não uma cópia linha a linha
das estruturas.

  Documento                                      Conteúdo
  ---------------------------------------------- ----------------------------------------------
  `OpenCircuit_Arquitetura_Tecnica_v0.2.md`      Decisões, alternativas e racional. Atualizar
                                                 apenas para decisões irreversíveis.

  `ARCHITECTURE.md`                              Mapa de uma página e regras de dependência.

  `SIMULATION.md`                                Ordem do tick, temporizadores, térmico,
                                                 política de falha e formato do trace.

  `FORMAT.md`                                    Schema, versões, migrações e round-trip.

  `ROADMAP.md`                                   Marcos, critérios de saída e escopo.

  Código (`circuit-model`, `circuit-sim`)        Tipos, assinaturas e comportamento
                                                 executável.

ADRs só são escritos para decisões irreversíveis. Detalhes de undo/redo,
determinismo, formato JSON genérico e nomes de campos vivem no código,
testes e documentos específicos. Ver §22.

------------------------------------------------------------------------

## 20. Roadmap e critérios de saída

O roadmap é organizado por incrementos verificáveis, não por camadas
técnicas. Cada marco só começa quando o anterior tem um critério de saída
observável. A ordem existe para reduzir risco: primeiro a semântica do
motor, depois a interface.

### 20.1 M0 --- Fundação (concluído)

Entregue:

-   workspace Rust com edition 2024, resolver 3, `Cargo.lock` versionado e
    toolchain fixada;
-   `circuit-domain` e `circuit-diagnostics` com IDs, dispositivo, símbolo,
    terminal, fio, junção, condutores, perfil de alimentação e diagnósticos
    estruturais;
-   `xtask` com `check-architecture`;
-   CI com formatação, clippy, testes, `cargo-deny` e verificação de
    camadas;
-   licenças, README, documentos de apoio e placeholders rastreados.

Critério atendido: `cargo fmt --all -- --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace --locked`, `cargo
deny check` e `cargo run -p xtask -- check-architecture` passam.

O M0 é uma base de contratos, não a arquitetura final de crates. A
consolidação em `circuit-model` e `circuit-sim` é o primeiro passo do M1.

### 20.2 M1 --- Vertical slice headless

**Objetivo:** provar o motor completo em uma partida direta e em um
reversor, sem GUI.

Escopo:

-   consolidar `circuit-domain` + `circuit-diagnostics` em `circuit-model`;
-   criar `circuit-sim` com os módulos `topology`, `electrical`, `logic`,
    `runtime` e `trace`;
-   catálogo embutido em Rust para os componentes do slice;
-   goldens de partida direta e reversor, com fixtures em memória;
-   criar `circuit-project` com JSON v1;
-   criar `apps/cli` com `validate`, `simulate` e `test`;
-   testes de TON/TOF, térmico, curto, falta para PE, `next_id` e
    round-trip.

Critérios de saída:

1.  O workspace tem exatamente cinco membros: `circuit-model`,
    `circuit-sim`, `circuit-project`, `apps/cli` e `xtask`.
2.  `circuit-model` não depende de nenhum crate do workspace;
    `circuit-sim` depende apenas de `circuit-model`; `circuit-project`
    depende apenas de `circuit-model`.
3.  A partida direta passa no golden com o log de mudanças esperado:
    S1 pressionado energiza K1 no mesmo tick; o contato de selo fecha no
    tick seguinte; soltar S1 mantém K1; S2 abre e desenergiza K1 no mesmo
    tick; os contatos principais abrem no tick seguinte.
4.  O térmico passa no golden: `trip` abre o NF no tick seguinte; `reset`
    fecha o NF no tick seguinte, mas não reenergiza K1; S1 reenergiza.
5.  O reversor passa no golden do caso simultâneo: no tick `n` as duas
    bobinas energizam; no tick `n+1` os dois NF de intertravamento abrem,
    as bobinas caem, mas os dois contatos principais ainda estão fechados e
    o solver detecta `OC-ELEC-001 short-circuit`; o snapshot de `n+1` tem
    `faulted = true`; nenhum tick posterior executa sem `reset_fault()`.
6.  TON e TOF passam nos testes de fronteira do §11.3, incluindo
    cancelamento no tick em que o preset seria atingido.
7.  `save`/`load` faz round-trip de um documento v1; `next_id` inválido,
    campo desconhecido e `schema_version` maior são recusados com erros
    distintos.
8.  O snapshot público contém apenas IDs de documento e `ConductorMask`;
    nenhum índice interno aparece em JSON ou em assinatura pública.
9.  A CLI retorna 0 em sucesso, 1 em erro de validação/compilação, 2 em
    falha elétrica travada e 3 em erro de uso.
10. `cargo check -p circuit-sim --target wasm32-unknown-unknown` passa,
    mesmo que a aplicação web não exista.
11. `cargo test --workspace --locked`, fmt, clippy, deny e arquitetura
    passam em Linux; a matriz Windows/macOS continua verde.
12. Os documentos `ARCHITECTURE.md`, `SIMULATION.md`, `FORMAT.md` e
    `ROADMAP.md` descrevem o mesmo contrato que o código.

O M1 não termina com "o código compila". Termina quando os dois goldens
provam a semântica temporal e a política de falha.

### 20.3 M2a --- Viewer somente leitura

**Objetivo:** validar o contrato `Schematic + SimSnapshot → renderer` antes
de investir em edição.

Escopo:

-   `circuit-renderer` com cena neutra;
-   `apps/desktop` com janela egui/eframe;
-   carregar `.ocircuit` via `circuit-project`;
-   compilar e simular via `circuit-sim`;
-   play, pause, step, reset e velocidade;
-   cores por máscara de condutor, contatos fechados, bobinas energizadas,
    motor e térmico;
-   painel de diagnósticos de compilação e runtime.

Critérios de saída:

1.  O viewer abre o fixture de partida direta e mostra o circuito sem
    editar o documento.
2.  Ao dar play, K1 e o motor mudam nos ticks esperados; ao pausar, o
    snapshot não muda.
3.  A 1 ms por tick e 60 fps, o viewer chama `step_n(16, ...)` e um único
    `snapshot()` por frame.
4.  O renderer não depende de `circuit-sim`; toda a informação vem de
    `circuit-model`.
5.  Diagnósticos de compilação e runtime aparecem separados e ordenados.
6.  Um smoke test automatizado abre o fixture, avança N ticks e verifica
    que K1 e o motor aparecem no snapshot.
7.  Benchmarks iniciais com 100 e 500 componentes registram tempo de
    cena, hit-test e simulação; não há otimização especulativa.

### 20.4 M2b --- Editor

**Objetivo:** completar o MVP de edição sobre o viewer.

Escopo:

-   `circuit-commands` com comandos transacionais e undo/redo;
-   `circuit-editor` com seleção, hit-test, inserção, movimentação,
    rotação, exclusão, fios e junções;
-   paleta de componentes a partir do catálogo;
-   inspector de propriedades e parâmetros;
-   salvar e reabrir;
-   testes de fluxo com `egui_kittest`.

Critérios de saída:

1.  Inserir, selecionar, mover, rotacionar e excluir componentes.
2.  Desenhar fios com waypoints; cruzar fios não conecta; criar junção
    explícita conecta.
3.  Undo/redo reverte e reaplica cada operação; uma operação inválida não
    deixa o documento parcialmente alterado.
4.  Salvar, fechar e reabrir preserva o documento e o resultado da
    compilação.
5.  Editar o documento invalida a simulação e exige recompilação
    explícita; não há mistura entre documento editado e circuito
    compilado antigo.
6.  Os fluxos essenciais passam em `egui_kittest`: selecionar, mover,
    rotacionar, excluir, conectar, desfazer, refazer, salvar e reabrir.
7.  O MVP do §2.3 está funcional, com a ressalva de que o solver continua
    discreto e não calcula tensão, corrente ou torque.

### 20.5 M3 --- Circuitos avançados e exportação

**Objetivo:** ampliar o catálogo e a saída documental sem quebrar o núcleo.

Escopo:

-   contatos reversíveis (`Changeover`) e intertravamento completo;
-   partida estrela-triângulo com motor de seis terminais e enrolamentos;
-   múltiplas seções de alimentação galvanicamente separadas;
-   exportação SVG a partir da cena neutra;
-   decisão sobre PDF após spike;
-   benchmarks de compilação, simulação e renderização em documentos
    maiores.

Critérios de saída:

1.  Estrela-triângulo executa sem sobreposição entre estrela e triângulo e
    com temporização verificável.
2.  O motor de seis terminais distingue estrela, triângulo e direção.
3.  Seções com perfis distintos não geram falso `incompatible-supplies`.
4.  SVG é determinístico, vetorial e testado por comparação normalizada.
5.  PDF é escolhido ou adiado com justificativa registrada; não bloqueia o
    restante do marco.
6.  Documentos de 2.000 componentes permanecem utilizáveis no viewer.

### 20.6 M4 --- Ecossistema

Fora do compromisso de MVP:

-   aplicação web/WASM;
-   plugins WASM;
-   importação de formatos proprietários;
-   Modbus, OPC UA e MQTT;
-   colaboração em tempo real;
-   biblioteca externa de símbolos e catálogo de terceiros.

Cada item exige um spike e um ADR próprio. Nenhum deles deve influenciar a
estrutura de M1 além da portabilidade do núcleo.

### 20.7 Regra de avanço

Um marco não avança por data. Avança quando:

-   todos os critérios de saída estão verdes;
-   não há regressão nos marcos anteriores;
-   os documentos afetados foram reconciliados;
-   as decisões novas foram registradas no documento ou em um ADR.

------------------------------------------------------------------------

## 21. Riscos e mitigações

### 21.1 Riscos técnicos

  Risco                                      Impacto   Mitigação
  ------------------------------------------ --------- ----------------------------------------------
  Drift entre documento e código             Alto      Código é a fonte de verdade a partir de M1;
                                                       documentos guardam decisões e invariantes;
                                                       mudança de contrato atualiza código, testes e
                                                       documentos no mesmo PR.

  GUI atrasar ou dominar o cronograma        Alto      Viewer somente leitura em M2a; renderer puro;
                                                       `egui_kittest`; motor entregue antes da GUI.

  Escopo escorregar para solver analógico    Alto      Modelo discreto explícito; cargas em série e
                                                       estrela flutuante diagnosticadas; MNA só
                                                       depois do MVP.

  Política de falha esconder oscilação       Médio     Golden do reversor registra o primeiro curto;
                                                       `faulted` no snapshot; `reset_fault`; um modo
                                                       de análise sem latch pode ser adicionado sem
                                                       mudar o padrão.

  Fronteira de timer/entrada ambígua         Médio     Ordem do tick fixa; entradas antes da
                                                       comparação; testes de cancelamento no limite.

  Catálogo e símbolos divergirem             Médio     `model_version`; validação de terminais;
                                                       proveniência e licença por símbolo.

  Armadilha dos forks do CADCraft            Médio     Spike antes de adotar; reproduzir o problema;
                                                       fixar revisão; testar a família egui/winit
                                                       inteira.

  PDF virar dependência pesada               Baixo     SVG primeiro; PDF como spike; não bloquear
                                                       M1/M2.

  Persistência incompatível                  Médio     `schema_version`; `deny_unknown_fields`;
                                                       recusa de versão maior; migrações testadas;
                                                       round-trip só na mesma versão.

  16 ticks por frame com snapshot por tick   Médio     `step_n` + um snapshot por frame; benchmarks
                                                       em M2a; culling e invalidação seletiva.

  Excesso de cerimônia para um mantenedor    Médio     Cinco crates, quatro ADRs, marcos com
                                                       critérios; nenhuma abstração sem segundo
                                                       consumidor.

  Promessa de WASM cedo demais               Baixo     Núcleo portátil verificado no CI; aplicação
                                                       web só em M4, após spike.
  -----------------------------------------------------------------------

### 21.2 Riscos de produto

-   **Público pequeno:** o MVP precisa resolver a partida direta muito bem
    antes de competir em recursos com CADe SIMU. O diferencial é o modelo
    correto e o diagnóstico, não a quantidade de símbolos.
-   **Expectativa de simulação física:** a interface deve deixar claro que
    o resultado é lógico/discreto. Não mostrar tensão, corrente ou torque
    que o solver não calcula.
-   **Licenciamento de símbolos:** não copiar bibliotecas externas. Criar
    símbolos próprios com proveniência e revisão.
-   **Complexidade do editor:** o editor é o maior custo de UI. O viewer
    reduz o risco ao validar a cena e o snapshot antes de comandos e
    undo/redo.

------------------------------------------------------------------------

## 22. Decisões de arquitetura (ADRs)

Quatro decisões justificam um ADR. As demais são detalhes de implementação
e vivem no código, nos testes e nos documentos específicos.

### 22.1 ADR-001 --- Stack e plataforma

**Status:** aceito.

**Contexto:** o MVP é desktop, a GUI ainda não começou e o núcleo precisa
ser portátil. O CADCraft é uma referência de editor CAD em Rust com
egui/eframe, mas usa forks e patches que não devem ser copiados sem
investigação.

**Decisão:**

-   Rust com edition 2024, resolver 3 e toolchain fixada;
-   egui/eframe para a GUI desktop;
-   núcleo sem dependência de egui, eframe ou APIs de sistema operacional;
-   desktop Linux, Windows e macOS no MVP;
-   WebAssembly como requisito de portabilidade do núcleo, não como
    entrega do MVP;
-   GUI somente em M2, depois do motor headless.

**Consequências:**

-   O primeiro incremento é testável por CLI e não depende de janela.
-   A versão de egui/eframe é fixada no momento da implementação, não
    copiada deste documento.
-   Código de UI fica restrito a `apps/desktop`.
-   A aplicação web não é prometida; o núcleo é verificado no alvo WASM.

**Alternativas rejeitadas:**

-   Tauri ou frontend web no MVP: adiciona uma fronteira JS e mais partes
    móveis antes de o motor estar provado.
-   Qt ou outro toolkit nativo: licenciamento, bindings e empacotamento
    mais complexos para o ganho inicial.
-   Renderer próprio: custo alto e sem relação com o risco principal.
-   Web primeiro: atrasa o feedback sobre a semântica do motor.

### 22.2 ADR-002 --- Identidade e formato do documento

**Status:** aceito.

**Contexto:** a v0.2 misturava `Tag`, `DeviceKind` e identidade de
dispositivo; símbolos e terminais não tinham uma separação estável; e o
formato oscilava entre IDs opacos e strings. Isso contaminava topologia,
snapshot e persistência.

**Decisão:**

-   `DeviceId`, `SymbolId`, `WireId`, `JunctionId` e `SupplyId` são
    newtypes de `u64`;
-   o documento persiste `next_id`, um contador global; IDs nunca são
    reutilizados;
-   `Device` guarda `reference`, `model_id`, `model_version` e
    `parameters`;
-   terminais pertencem ao `SymbolModel`, não ao dispositivo;
-   o documento não guarda `DeviceKind`;
-   o formato é JSON `.ocircuit` com `schema_version`;
-   v1 usa `deny_unknown_fields`, recusa versão maior e garante round-trip
    apenas dentro da mesma versão.

**Consequências:**

-   IDs são determinísticos, legíveis e não exigem `uuid`/`getrandom` no
    WASM.
-   A alocação é central e exige validação de `next_id` na carga.
-   O catálogo evolui por `model_version`; o documento referencia uma
    versão exata.
-   Adicionar campo exige incrementar a versão e escrever migração.
-   Não há promessa de preservar campos desconhecidos.

**Alternativas rejeitadas:**

-   UUID: configuração extra, identificadores ilegíveis e nenhum benefício
    para um arquivo local.
-   Tags string como identidade: duplicates e colisões sem controle.
-   Índices posicionais de pino: instáveis sob rotação, edição e
    reordenação.
-   `DeviceKind` persistido: acopla o documento ao catálogo e impede
    modelos de terceiros.
-   Ignorar campos desconhecidos: mascara incompatibilidade de versão.

### 22.3 ADR-003 --- Modelo elétrico discreto

**Status:** aceito para M1; reavaliar apenas com evidência de um solver
resistivo.

**Contexto:** o MVP precisa de continuidade, curto, falta para PE e
energização de cargas, não de tensão, corrente ou transitórios. A v0.2
refazia o Union-Find dos fios a cada tick e usava "potencial virtual" como
solução genérica, o que podia declarar redes inválidas como energizadas.

**Decisão:**

-   fios e junções formam nets de cobre estáticas, compiladas uma vez;
-   por tick, apenas os switches fechados unem nets de cobre;
-   cada net dinâmica carrega um `ConductorMask` (`u8`) com as funções de
    condutor presentes;
-   curto é definido por perfil de alimentação, não por "qualquer
    diferença";
-   carga é condutiva quando os condutores requeridos alcançam nets
    distintas dos seus terminais;
-   tensão e frequência nominais servem apenas para compatibilidade de
    bobina/carga, não para o solver;
-   carga em série, estrela flutuante e topologias não suportadas geram
    diagnóstico de limitação em runtime;
-   não há MNA nem propagação de potencial no MVP.

**Consequências:**

-   O custo por tick é proporcional ao número de nets e switches, não ao
    número de fios.
-   O resultado é determinístico e explicável.
-   O modelo não calcula divisão de tensão, corrente ou torque; a interface
    deve dizer isso.
-   O caminho de evolução para um solver resistivo fica aberto, mas não é
    pré-requisito do MVP.

**Alternativas rejeitadas:**

-   Potencial virtual genérico: falso positivo em redes sem fonte.
-   Refazer o Union-Find dos fios por tick: custo e ambiguidade.
-   `fixedbitset`/`petgraph` na base: dependências sem necessidade
    demonstrada.
-   Solver analógico no MVP: escopo e custo incompatíveis com o objetivo.

### 22.4 ADR-004 --- Tick, snapshot e política de falha

**Status:** aceito.

**Contexto:** a v0.2 misturava o estado usado na solução com o estado
calculado pela lógica. O snapshot podia mostrar um contato fechado que
ainda não havia participado da solução. Também não havia política clara
para curto, falta para PE, temporizadores e térmico.

**Decisão:**

-   o runtime mantém `current` e `next`;
-   `solve(current)` → lógica → `next` → `current = next`;
-   entradas externas são aplicadas em `current` na fronteira do tick;
-   o snapshot é construído antes do commit e representa a rede
    efetivamente resolvida;
-   o trace é um log de mudanças, não um snapshot por tick;
-   curto, falta para PE e fontes incompatíveis travam a simulação após o
    snapshot do tick;
-   incompatibilidade de bobina, carga curto-circuitada e carga em série
    não travam;
-   TON/TOF comparam `elapsed_next >= preset` depois de processar a
    entrada;
-   o térmico é um latch; `trip` vence `reset` no mesmo tick; reset não
    reenergiza o contator.

**Consequências:**

-   Há um atraso de um tick em contatos controlados por lógica; isso é
    scan, não tempo mecânico.
-   O golden do reversor termina no primeiro curto, não em oscilação
    indefinida.
-   A UI precisa manter uma entrada momentânea ativa durante o lote de
    ticks do frame.
-   `reset_fault()` rearma o runtime, mas corrigir o circuito exige
    recompilar.

**Alternativas rejeitadas:**

-   Iterar até ponto fixo: complexidade e risco de não terminação.
-   Snapshot depois do commit: mistura dois instantes lógicos.
-   Continuar após curto: produz estado fictício.
-   `dt` por chamada: replay não reproduzível.
-   Modelo térmico com corrente: fora do escopo discreto do MVP.

### 22.5 O que não vira ADR

-   Nomes de campos e módulos.
-   Estrutura interna do Union-Find.
-   Formato exato do JSON além do que afeta compatibilidade.
-   Estratégia de undo/redo.
-   Escolha de biblioteca de PDF antes do spike.
-   Detalhes de renderização e temas.

Esses itens mudam com frequência e pertencem ao código, aos testes e aos
documentos de apoio.

------------------------------------------------------------------------

## 23. Primeiro incremento executável

### 23.1 Ordem de ataque

  Passo   Entrega                                              Verificação
  ------- ---------------------------------------------------- ---------------------------------
  0       Consolidar `circuit-model` e criar o esqueleto de    16 testes de M0 portados; workspace
          `circuit-sim`, `circuit-project` e `apps/cli`.       verde.

  1       Documento, catálogo mínimo, `Inputs` e               Documento de partida direta compila
          `SimSnapshot` em `circuit-model`.                    sem erros.

  2       `topology::compile` com `CompileOutput`, nets de     Fingerprint estável; diagnóstico de
          cobre, switches, fontes, cargas e tabela de sinais.  modelo ausente.

  3       `electrical::solve` com Union-Find sobre nets de     Testes de máscara, curto por perfil,
          cobre, máscaras, curto e cargas.                     falta para PE e carga condutiva.

  4       `logic` com contatos, bobinas, TON/TOF, térmico e    Testes de borda e latch.
          entradas.

  5       `runtime` com `current`/`next`, ordem do tick,       Trace de mudanças e snapshot em IDs
          snapshot, change log e política de falha.            de documento.

  6       Golden da partida direta, com fixture em memória.    Trace esperado, incluindo selo,
                                                               S2 e térmico.

  7       Golden do reversor, incluindo S1+S2 simultâneos.      Curto em `n+1`, latch e snapshot
                                                               `faulted`.

  8       `circuit-project` com JSON v1, `next_id` e           Round-trip e recusa de versão/campo.
          validação.

  9       `apps/cli` com `validate`, `simulate` e `test`.      Códigos de saída 0/1/2/3.

  10      Revisão de contrato e documentação.                  Nenhum termo antigo; docs
                                                               reconciliados.
  -----------------------------------------------------------------------

A ordem não é uma sugestão. O passo 6 não começa antes de o passo 5
produzir um trace confiável; o passo 7 não começa antes de o passo 6
fechar. O JSON e a CLI vêm depois dos goldens porque o critério de sucesso
é a semântica do motor, não o formato de arquivo. A GUI não entra em nenhum
passo.

### 23.2 Estrutura inicial

``` text
crates/circuit-model/src/
├── lib.rs
├── ids.rs
├── document.rs
├── catalog.rs
├── conductor.rs
├── diagnostics.rs
├── inputs.rs
└── snapshot.rs

crates/circuit-sim/src/
├── lib.rs
├── topology/
│   ├── mod.rs
│   ├── compile.rs
│   ├── netlist.rs
│   └── signals.rs
├── electrical/
│   ├── mod.rs
│   ├── union_find.rs
│   └── solve.rs
├── logic/
│   ├── mod.rs
│   ├── contacts.rs
│   ├── coils.rs
│   ├── timers.rs
│   └── thermal.rs
├── runtime/
│   ├── mod.rs
│   ├── simulator.rs
│   └── fault.rs
└── trace.rs

crates/circuit-project/src/
├── lib.rs
├── json.rs
└── migrate.rs

apps/cli/src/
└── main.rs
```

O catálogo embutido do M1 é código Rust (`catalog::builtin()`), versionado
junto com o crate. Formato externo de catálogo é spike de M3/M4; não
introduzir um segundo formato antes de existir um consumidor real.

### 23.3 Definição de pronto do M1

-   [ ] Cinco crates no workspace; nenhum placeholder em `members`.
-   [ ] `circuit-model` sem dependências do workspace.
-   [ ] `circuit-sim` depende apenas de `circuit-model`.
-   [ ] Nenhum crate de núcleo depende de egui/eframe.
-   [ ] `CompileOutput` implementado; erros e avisos coexistem sem
      resultado parcial.
-   [ ] Nets de cobre estáticas; switches aplicados por tick.
-   [ ] `ConductorMask` e regras de curto por perfil.
-   [ ] Cargas avaliadas por pares/trios compatíveis; sem potencial
      virtual.
-   [ ] `current`/`next` e ordem do tick implementados.
-   [ ] Snapshot somente com IDs de documento.
-   [ ] Change log sem snapshot por tick.
-   [ ] TON/TOF e térmico com testes de fronteira.
-   [ ] Política de falha com latch e `reset_fault`.
-   [ ] `next_id` validado; JSON v1 com round-trip e recusa de versão.
-   [ ] CLI com os quatro códigos de saída.
-   [ ] Goldens de partida direta e reversor.
-   [ ] `cargo test`, fmt, clippy, deny, arquitetura e WASM do núcleo
      verdes.
-   [ ] `ARCHITECTURE.md`, `SIMULATION.md`, `FORMAT.md` e `ROADMAP.md`
      reconciliados.

### 23.4 O que não fazer neste incremento

-   Não criar `circuit-renderer`, `circuit-editor`, `circuit-commands` ou
    `circuit-export`.
-   Não adicionar egui/eframe ao workspace antes do M2a.
-   Não implementar `Changeover`, estrela-triângulo ou motor de seis
    terminais.
-   Não implementar solver resistivo, MNA ou cálculo de corrente.
-   Não adicionar formato externo de catálogo.
-   Não implementar aplicação web/WASM.
-   Não otimizar Union-Find, alocação ou renderização sem medição.

------------------------------------------------------------------------

## 24. Spikes pendentes

### 24.1 Forks do CADCraft

**Pergunta:** algum patch de `egui`, `winit`, `krilla`, `hayro-syntax` ou
`hayro-jbig2` é necessário para o OpenCircuit?

**Método:**

1.  Fixar a revisão do CADCraft analisada.
2.  Listar cada `[patch.crates-io]` e o problema que ele resolve.
3.  Reproduzir o problema com a versão upstream no OpenCircuit.
4.  Verificar licença, manutenção, histórico e portabilidade WASM.
5.  Testar a família egui/winit completa, sem misturar upstream e fork.
6.  Registrar a decisão e o responsável pela atualização.

**Saída:** nota curta com revisão fixada, justificativa por patch e decisão
de adotar ou não. Não copiar o bloco de patches do CADCraft por
similaridade.

### 24.2 Exportação PDF

**Pergunta:** qual biblioteca gera PDF vetorial com texto, fontes
incorporadas e licença compatível?

**Candidatos:** `krilla` e alternativas do ecossistema Rust. Comparar:

-   licença MIT/Apache-2.0;
-   suporte a texto, vetores e fontes;
-   qualidade de símbolos e tabelas;
-   manutenção e versão;
-   portabilidade WASM, se necessária.

**Saída:** decisão registrada ou adiamento explícito. SVG é o primeiro
formato e não depende desse spike.

### 24.3 Roteamento e hit-test do editor

**Pergunta:** qual estratégia de roteamento e hit-test atende ao grid sem
tornar o editor frágil?

Investigar:

-   waypoints intermediários e reconexão após mover símbolo;
-   junção explícita versus interseção;
-   seleção de fio e de terminal;
-   snap, zoom e pan;
-   custo de hit-test com muitos objetos.

**Saída:** protótipo descartável em M2b, com testes de fluxo. Não criar
`circuit-editor` antes do viewer.

### 24.4 UI web/WASM

**Pergunta:** eframe web é suficiente ou a aplicação web precisa de um
frontend separado?

Investigar:

-   tamanho do bundle e tempo de inicialização;
-   entrada de teclado, mouse e toque;
-   abertura e salvamento de arquivos no navegador;
-   acesso a `wgpu`/WebGL e fallback;
-   limitações de threads e `getrandom`.

**Saída:** decisão de M4. O núcleo continua portátil e verificado no CI;
a aplicação web não é prometida antes disso.

### 24.5 Catálogo externo

**Pergunta:** quando o catálogo embutido em Rust deixa de bastar?

Gatilhos:

-   símbolos de terceiros;
-   catálogo editável sem recompilar;
-   múltiplas bibliotecas por projeto;
-   versionamento independente do aplicativo.

**Saída:** formato, proveniência, validação e migração definidos em ADR
próprio. Até lá, `catalog::builtin()` é a fonte de verdade.

------------------------------------------------------------------------

## 25. Conclusão

A v0.2.1 não é uma versão "mais completa" da v0.2. É uma versão mais
estreita e mais precisa. As decisões que sustentam o projeto são cinco:

1.  **Dispositivo, símbolo e terminal são entidades distintas.** O
    contator K1 tem uma identidade, várias representações e terminais
    declarados pelo modelo do símbolo.
2.  **O cobre é estático; os switches são dinâmicos.** Fios e junções são
    compilados uma vez; cada tick apenas une nets de cobre pelos switches
    fechados.
3.  **O tick tem dois estados.** `current` é resolvido; `next` é
    calculado; o commit é explícito. O snapshot representa a rede que foi
    realmente resolvida.
4.  **O snapshot e o trace falam a língua do documento.** IDs estáveis e
    `ConductorMask`; nenhum índice interno vaza para a UI ou para o JSON.
5.  **Falha elétrica é falha fechada.** Curto, falta para PE e fontes
    incompatíveis travam a simulação após o snapshot do tick.

O primeiro incremento não é a GUI. É uma partida direta headless com Q1,
F1, S1, S2, K1 e M1, seguida de um reversor que prova o caso simultâneo.
Quando esses dois goldens passarem, o projeto terá provado o que é difícil:
a semântica temporal, a identidade do modelo e a política de falha. O
editor, o viewer e a exportação são camadas sobre esse núcleo, não
substitutos dele.

O critério de sucesso do M1 é simples: um engenheiro consegue carregar um
`.ocircuit`, rodar um cenário pela CLI, inspecionar um log de mudanças
legível e confiar que o mesmo documento, o mesmo estado inicial e as
mesmas entradas produzem o mesmo resultado.

------------------------------------------------------------------------

## Apêndice A --- Checklist de aprovação

### A.1 Decisões fechadas

-   [x] Device ≠ SymbolInstance ≠ Terminal.
-   [x] Terminais pertencem ao modelo do símbolo; chaves estáveis, nunca
    índices posicionais.
-   [x] `Device` guarda `model_id` + `model_version`; `DeviceKind` não é
    identidade persistida.
-   [x] `next_id` persistido; IDs `u64` nunca reutilizados; sem `uuid`.
-   [x] `compile()` devolve `CompileOutput`; `circuit` só existe sem erro.
-   [x] Nets de cobre estáticas; switches aplicados por tick.
-   [x] `SignalTable`/`DeviceSignals` na topologia, não no runtime.
-   [x] Snapshot com IDs de documento e `ConductorMask`.
-   [x] Commit `current`/`next` e ordem do tick.
-   [x] Entradas `Momentary`, `Maintained` e `Pulse`.
-   [x] Fronteira de TON/TOF e precedência de `trip` sobre `reset`.
-   [x] Política de falha com latch e `reset_fault`.
-   [x] Trace como change log, não snapshot por tick.
-   [x] Cinco crates iniciais; GUI em M2.
-   [x] JSON v1 com `deny_unknown_fields`, recusa de versão maior e
    round-trip na mesma versão.
-   [x] Viewer somente leitura antes do editor.
-   [x] Quatro ADRs.

### A.2 Evidências pendentes

-   [ ] Golden da partida direta em M1.
-   [ ] Golden do reversor com S1+S2 simultâneos em M1.
-   [ ] Round-trip e recusa de schema em M1.
-   [ ] CLI com códigos de saída em M1.
-   [ ] Benchmark do viewer em M2a.
-   [ ] Fluxos de undo/redo em `egui_kittest` em M2b.
-   [ ] Spike dos forks do CADCraft em M2.
-   [ ] Spike de PDF em M3.
-   [ ] Spike de UI web/WASM em M4.

### A.3 Aprovação

A arquitetura está aprovada para iniciar o M1 quando:

-   a seção A.1 estiver integralmente marcada;
-   o primeiro incremento tiver um responsável e uma sequência de passos
    acordada;
-   não houver contradição conhecida entre este documento, os documentos
    de apoio e o código;
-   o repositório estiver limpo o suficiente para que a migração de crates
    seja revisável.

Os itens de A.2 são evidências de implementação, não pré-condições para
começar. Eles devem ser fechados nos marcos correspondentes.

------------------------------------------------------------------------

## Apêndice B --- Referências

### B.1 Documentos do projeto

-   `ideia.md` --- análise técnica da v0.1, com o racional de
    dispositivo/símbolo, estado atual/próximo e críticas ao modelo
    elétrico.
-   `OpenCircuit_Arquitetura_Tecnica_v0.2.md` --- este documento.
-   `ARCHITECTURE.md` --- mapa de uma página e regras de dependência.
-   `SIMULATION.md` --- ordem do tick, temporizadores, térmico, política
    de falha e formato do trace.
-   `FORMAT.md` --- schema, versionamento, migrações e round-trip.
-   `ROADMAP.md` --- marcos, escopo e critérios de saída.

### B.2 Rust e ferramentas

-   The Rust Programming Language, edition 2024:
    <https://doc.rust-lang.org/edition-guide/rust-2024/>
-   Cargo resolver:
    <https://doc.rust-lang.org/cargo/reference/resolver.html>
-   `cargo-deny`: <https://embarkstudios.github.io/cargo-deny/>
-   `proptest`: <https://crates.io/crates/proptest>
-   `insta`: <https://crates.io/crates/insta>
-   `wasm32-unknown-unknown`:
    <https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html>

### B.3 GUI e referências CAD

-   egui: <https://crates.io/crates/egui>
-   eframe: <https://crates.io/crates/eframe>
-   `egui_kittest`: <https://crates.io/crates/egui_kittest>
-   CADCraft --- repositório público de referência para editor CAD em
    Rust. A revisão consultada, os patches e a configuração devem ser
    fixados no spike do §24.1; não copiar o manifesto sem essa análise.
-   `krilla`: <https://crates.io/crates/krilla> (candidato a PDF, sujeito
    ao spike do §24.2).

### B.4 Normas e domínio elétrico

-   IEC 60617 --- símbolos gráficos para diagramas.
-   IEC 60204-1 --- equipamento elétrico de máquinas.
-   IEC 60947 --- aparelhos de manobra e comando de baixa tensão.
-   ABNT NBR 5410 --- instalações elétricas de baixa tensão, quando
    aplicável à jurisdição do usuário.

As normas são referências de vocabulário e de boas práticas. O OpenCircuit
não declara conformidade certificada com nenhuma delas sem revisão técnica
e jurídica apropriada.

------------------------------------------------------------------------

**Fim do documento.**
