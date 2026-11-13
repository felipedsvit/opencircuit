# OpenCircuit

## Documento Técnico Consolidado e Prospectivo

**Nome de trabalho:** OpenCircuit\
**Versão do documento:** 0.2 --- proposta consolidada\
**Data:** 13/11/2026\
**Status:** Arquitetura proposta; sujeita a validação por protótipos e
testes\
**Plataforma inicial:** Desktop --- Linux, Windows e macOS\
**Plataforma futura:** WebAssembly\
**Linguagem:** Rust\
**Interface:** egui/eframe\
**Licença pretendida:** MIT OR Apache-2.0

------------------------------------------------------------------------

## 1. Resumo executivo

OpenCircuit é uma proposta de editor e simulador de esquemas elétricos
de comando, com foco inicial em automação e painéis industriais. O
objetivo é oferecer uma alternativa open source a ferramentas como CADe
SIMU, começando por circuitos de comando e evoluindo de forma
incremental para circuitos de potência, documentação técnica, integração
industrial e extensibilidade.

A arquitetura proposta separa o documento elétrico, o catálogo de
dispositivos, a compilação da topologia, os modelos elétricos e lógicos,
o runtime de simulação, a edição gráfica, a renderização e a
persistência.

A decisão central é tratar o circuito como um modelo de domínio
independente da interface. O editor apresenta e altera esse modelo; o
compilador o transforma em uma representação executável; o simulador
calcula os estados; e a renderização apresenta o documento e os
resultados da simulação.

**Princípio arquitetural:** OpenCircuit não deve ser um editor de
desenhos que tenta inferir eletricidade a partir de pixels. Deve ser um
modelo de circuito que possui uma representação gráfica.

A versão 0.2 consolida a arquitetura inicial e corrige pontos que
poderiam causar inconsistências futuras:

-   Separação entre dispositivo, símbolo gráfico e terminal.
-   Identificadores estáveis de terminais no formato de projeto.
-   Distinção entre identificação de condutores e potencial elétrico.
-   Semântica temporal explícita para cada tick.
-   Snapshot coerente com o instante resolvido.
-   Limites claros do solver discreto.
-   Separação entre diagnósticos estruturais, semânticos e de execução.
-   Verificação automatizada das regras de dependência entre crates.

O MVP será um editor desktop capaz de desenhar, salvar, carregar e
simular uma partida direta de motor, incluindo circuito de selo,
contatos auxiliares, proteção térmica e sinalização. A arquitetura deve
permitir ampliar o catálogo sem acoplar o núcleo à GUI.

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
-   Botões NA e NF.
-   Contatos auxiliares NA e NF.
-   Bobinas de contatores e relés.
-   Temporizadores TON e TOF.
-   Lâmpadas e buzzer.
-   Representação simplificada de motor trifásico como carga.
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
preliminar. Seus resultados não substituem projeto elétrico
profissional, análise de risco, ensaios, comissionamento, validação de
proteção ou atendimento às normas aplicáveis. O software não deverá
alegar certificar instalações ou garantir segurança operacional.

------------------------------------------------------------------------

## 3. Princípios arquiteturais

1.  **Domínio independente:** nenhum crate do núcleo depende de egui,
    eframe ou APIs de sistema operacional.
2.  **Modelo explícito:** dispositivos, símbolos, terminais, fios,
    junções e fontes são entidades distintas.
3.  **Compilação do circuito:** o documento é compilado para uma
    representação executável antes da simulação.
4.  **Determinismo:** com o mesmo circuito compilado, estado inicial,
    sequência de entradas e configuração temporal, o runtime deve
    produzir o mesmo trace lógico.
5.  **Tempo virtual:** o motor não lê o relógio do sistema.
6.  **Snapshot coerente:** cada snapshot representa um único instante
    lógico definido.
7.  **Persistência versionada:** o formato de projeto tem schema_version
    e migrações.
8.  **Diagnóstico estruturado:** erros têm códigos estáveis e
    referências aos elementos envolvidos.
9.  **Testabilidade headless:** toda a simulação pode ser executada sem
    GUI.
10. **Evolução incremental:** o MVP não deve simular grandezas físicas
    que o solver ainda não calcula.

------------------------------------------------------------------------

## 4. Arquitetura geral

### 4.1 Fluxo de dados

``` text
                 ┌──────────────────────┐
                 │   apps/desktop       │
                 │   egui / eframe      │
                 └──────────┬───────────┘
                            │ edição / visualização
                            ▼
                 ┌──────────────────────┐
                 │  circuit-editor      │
                 │ seleção, hit-test,   │
                 │ roteamento, interação│
                 └──────────┬───────────┘
                            │ comandos
                            ▼
                 ┌──────────────────────┐
                 │  circuit-domain      │
                 │ Schematic / Devices  │
                 │ Symbols / Wires      │
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │  circuit-topology    │
                 │ compile → Netlist    │
                 └──────────┬───────────┘
                            │
                            ▼
                 ┌──────────────────────┐
                 │ circuit-simulation   │
                 │ runtime / tick       │
                 └──────┬────────┬──────┘
                        │        │
                 ┌──────▼───┐ ┌──▼────────────┐
                 │electrical│ │ circuit-logic │
                 │ solver   │ │ devices/timers│
                 └──────────┘ └───────────────┘
                        │
                        ▼
                 ┌──────────────────────┐
                 │    SimSnapshot       │
                 └──────────┬───────────┘
                            │
              ┌─────────────┴──────────────┐
              ▼                            ▼
     ┌──────────────────┐        ┌──────────────────┐
     │ circuit-renderer │        │ circuit-project  │
     │ Scene neutra     │        │ JSON / migrate   │
     └─────────┬────────┘        └──────────────────┘
               ▼
     ┌──────────────────┐
     │ egui / SVG / PDF │
     └──────────────────┘
```

O diagrama é conceitual. A implementação deverá manter as dependências
unidirecionais descritas na seção 5.

### 4.2 Separação de responsabilidades

-   **Documento:** descreve o circuito e sua geometria.
-   **Catálogo:** descreve tipos de dispositivos, terminais, parâmetros
    e símbolos.
-   **Topologia:** valida e compila conexões em índices densos.
-   **Electrical:** resolve conectividade elétrica ideal e
    compatibilidade discreta.
-   **Logic:** avalia dispositivos, contatos, bobinas e temporizadores.
-   **Simulation:** orquestra o ciclo temporal e produz snapshots.
-   **Editor:** gerencia interação e comandos de edição.
-   **Renderer:** converte documento e snapshot em uma cena gráfica
    neutra.
-   **Project:** serializa, desserializa e migra documentos.
-   **CLI:** executa cenários, testes e exportações sem interface.

------------------------------------------------------------------------

## 5. Workspace Rust

### 5.1 Estrutura proposta

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
│   ├── desktop/
│   └── cli/
├── crates/
│   ├── circuit-domain/
│   ├── circuit-diagnostics/
│   ├── circuit-components/
│   ├── circuit-topology/
│   ├── circuit-electrical/
│   ├── circuit-logic/
│   ├── circuit-simulation/
│   ├── circuit-project/
│   ├── circuit-commands/
│   ├── circuit-editor/
│   ├── circuit-renderer/
│   ├── circuit-export/
│   └── circuit-io/              # futuro
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

### 5.2 Responsabilidade dos crates

  -----------------------------------------------------------------------
  Crate                               Responsabilidade
  ----------------------------------- -----------------------------------
  `circuit-domain`                    IDs, dispositivos, símbolos,
                                      terminais, fios, junções e
                                      documento

  `circuit-diagnostics`               Códigos, severidade, sujeitos e
                                      mensagens de diagnóstico

  `circuit-components`                Catálogo de dispositivos,
                                      terminais, parâmetros e símbolos

  `circuit-topology`                  Compilação do documento para
                                      netlist

  `circuit-electrical`                Conectividade ideal, condutores,
                                      fontes e solver discreto

  `circuit-logic`                     Botões, bobinas, contatos, relés e
                                      temporizadores

  `circuit-simulation`                Runtime, tick, estado, eventos,
                                      snapshots e replay

  `circuit-project`                   JSON, validação de schema e
                                      migrações

  `circuit-commands`                  Comandos de edição, undo e redo

  `circuit-editor`                    Seleção, hit-test, roteamento e
                                      estado de interação

  `circuit-renderer`                  Conversão para cena gráfica neutra

  `circuit-export`                    Exportação SVG e PDF

  `circuit-io`                        Adaptadores futuros de I/O
                                      industrial
  -----------------------------------------------------------------------

### 5.3 Regras de dependência

``` text
domain             → dependências externas mínimas
diagnostics        → domain
components         → domain
topology           → domain, components, diagnostics
electrical         → domain, topology, diagnostics
logic              → domain, topology
simulation         → domain, topology, electrical, logic, diagnostics
project            → domain, components, diagnostics
commands           → domain
editor             → domain, commands, topology
renderer           → domain, components
export             → renderer
io                 → interfaces de runtime / simulation
apps/desktop       → crates necessários + egui/eframe
apps/cli           → crates necessários + clap
```

Regras obrigatórias:

1.  `circuit-domain` não depende de crates do workspace.
2.  `simulation`, `electrical` e `logic` não dependem de editor,
    renderer, commands ou project.
3.  `editor` e `renderer` não dependem diretamente de `simulation`;
    recebem tipos de snapshot definidos no domínio ou em uma interface
    neutra.
4.  Nenhum crate `circuit-*` depende de egui ou eframe.
5.  Adaptadores de I/O não devem introduzir sockets, relógios ou
    dependências de sistema operacional no núcleo.
6.  O CI deve verificar essas regras com `cargo metadata` por meio de
    `xtask check-architecture`. `cargo-deny` complementa a verificação
    com políticas de dependências, fontes e licenças; não substitui a
    validação das camadas internas.

### 5.4 Cargo.toml de referência

As versões devem ser resolvidas e testadas no momento da implementação.
O exemplo abaixo é uma base, não uma garantia de compatibilidade de
todas as versões:

``` toml
[workspace]
resolver = "3"
members = [
    "apps/*",
    "crates/*",
    "xtask",
]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.85"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
anyhow = "1"
uuid = { version = "1", features = ["v4", "serde"] }
fixedbitset = "0.5"
petgraph = "0.8"
tracing = "0.1"
tracing-subscriber = "0.3"

egui = "0.37"
eframe = "0.37"

proptest = "1"
insta = "1"
tempfile = "3"
criterion = "0.6"
clap = { version = "4", features = ["derive"] }
```

**Nota sobre MSRV:** `rust-version` deve ser ajustado depois de
verificar a versão mínima exigida pelo conjunto real de dependências e
pelos alvos suportados. Não se deve copiar automaticamente o MSRV de
outro projeto. Fixar `rust-toolchain.toml`, versionar `Cargo.lock` e
executar CI com `--locked`.

### 5.5 Política de forks

A configuração inicial deve preferir crates do crates.io. Forks Git
somente serão adotados quando houver:

-   problema reproduzível;
-   necessidade que não possa ser atendida pelo upstream;
-   revisão de licença e segurança;
-   commit imutável fixado;
-   testes que demonstrem o benefício;
-   responsável pela manutenção e estratégia de atualização.

Se for adotado um fork de egui, toda a família de crates relacionada
deve ser mantida em revisões compatíveis. Não misturar tipos de upstream
e fork na mesma aplicação.

------------------------------------------------------------------------

## 6. Modelo de domínio

### 6.1 Separar dispositivo, símbolo e terminal

Um dispositivo representa o equipamento lógico ou físico. Um símbolo é
uma representação gráfica desse dispositivo. Um terminal é um ponto de
conexão estável.

Essa separação evita que uma bobina e seus contatos auxiliares sejam
tratados como dispositivos sem relação entre si.

``` rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WireId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JunctionId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReferenceDesignator(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TerminalKey(pub String);
```

Exemplo conceitual:

``` text
Device K1
├── Symbol: coil K1
├── Symbol: auxiliary NO contact K1
├── Symbol: auxiliary NC contact K1
└── Symbol: main contacts K1
```

A referência `K1` pode aparecer em diversos símbolos associados ao mesmo
`DeviceId`. A duplicidade deve ser detectada entre dispositivos
distintos, não entre símbolos do mesmo dispositivo.

### 6.2 Referências a terminais

``` rust
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

O arquivo de projeto armazena chaves estáveis. O compilador resolve
essas chaves para índices densos internos.

### 6.3 Dispositivos e tipos

``` rust
pub enum DeviceKind {
    Contactor,
    Relay,
    ThermalOverload,
    Timer,
    Pushbutton,
    Motor,
    Lamp,
    Buzzer,
    PowerSupply,
}
```

Os detalhes de terminais e comportamento devem ser definidos pelo
catálogo, não duplicados em vários lugares do código.

### 6.4 Tipos de símbolo

``` rust
pub enum SymbolKind {
    Coil,
    ContactNormallyOpen,
    ContactNormallyClosed,
    ChangeoverContact,
    Pushbutton,
    Motor,
    Lamp,
    Buzzer,
    SupplyTerminal,
}
```

### 6.5 Posição e rotação

``` rust
pub struct GridPoint {
    pub x: i32,
    pub y: i32,
}

pub enum QuarterTurn {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

pub struct SymbolInstance {
    pub id: SymbolId,
    pub device_id: DeviceId,
    pub kind: SymbolKind,
    pub position: GridPoint,
    pub rotation: QuarterTurn,
}
```

As coordenadas são unidades lógicas do documento, nunca pixels de tela.

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

Dois fios que apenas se cruzam visualmente não devem ser conectados
automaticamente. A conexão exige uma junção explícita ou uma regra de
interseção claramente definida e testada.

### 6.7 Condutores e alimentação

O enum inicialmente chamado `Potential` mistura nomes de condutores com
grandezas elétricas. A versão consolidada utiliza `Conductor` para
identificar a função do condutor:

``` rust
pub enum Conductor {
    L1,
    L2,
    L3,
    Neutral,
    ProtectiveEarth,
    DcPositive,
    DcNegative,
}
```

`ProtectiveEarth` não é uma fonte de alimentação e não deve energizar
cargas. Neutro e PE não são intercambiáveis. A ligação entre neutro e PE
depende do esquema de aterramento e do ponto de equipotencialização
configurado.

A configuração de alimentação deve ser explícita:

``` rust
pub struct SupplyProfile {
    pub id: SupplyId,
    pub kind: SupplyKind,
    pub nominal_voltage_mv: u32,
    pub frequency_millihz: Option<u32>,
    pub phase_sequence: Option<PhaseSequence>,
}

pub enum SupplyKind {
    Dc,
    SinglePhase,
    ThreePhase,
}

pub enum PhaseSequence {
    L1L2L3,
    L1L3L2,
}
```

Este modelo identifica o sistema e permite validar compatibilidade
discreta. Não calcula por si só valores instantâneos de tensão ou
corrente.

### 6.8 Documento

``` rust
pub struct Schematic {
    pub schema_version: u32,
    pub devices: Vec<Device>,
    pub symbols: Vec<SymbolInstance>,
    pub junctions: Vec<Junction>,
    pub wires: Vec<Wire>,
    pub supplies: Vec<SupplyProfile>,
}
```

A implementação deverá definir tipos concretos para `Device`, `SupplyId`
e demais estruturas, além de validação de IDs únicos.

------------------------------------------------------------------------

## 7. Catálogo de componentes

O catálogo é a fonte de verdade para:

-   terminais disponíveis;
-   nomes e identificadores estáveis dos terminais;
-   requisitos de conexão;
-   parâmetros padrão;
-   associação entre símbolo e dispositivo;
-   comportamento lógico;
-   geometria do símbolo;
-   versão do modelo do componente.

Interface conceitual:

``` rust
pub struct ComponentDefinition {
    pub kind: DeviceKind,
    pub terminals: Vec<TerminalDefinition>,
    pub parameters: Vec<ParameterDefinition>,
    pub symbols: Vec<SymbolDefinition>,
}

pub struct TerminalDefinition {
    pub key: TerminalKey,
    pub label: String,
    pub required: bool,
}

pub struct SymbolDefinition {
    pub id: String,
    pub terminals: Vec<SymbolTerminal>,
    pub geometry: SymbolGeometry,
    pub standard_reference: Option<String>,
}
```

O catálogo deverá ser versionado. Alterar o significado ou a chave de um
terminal exige migração ou compatibilidade explícita.

------------------------------------------------------------------------

## 8. Compilação da topologia

### 8.1 Pipeline

``` text
Schematic
   │
   ├── validar IDs
   ├── validar referências
   ├── validar símbolos e terminais
   ├── resolver endpoints
   ├── construir conectividade
   ├── resolver referências de dispositivos
   ├── atribuir índices densos estáveis
   └── emitir CompiledCircuit + diagnostics
```

Contrato:

``` rust
pub fn compile(
    schematic: &Schematic,
    catalog: &ComponentCatalog,
) -> Result<CompiledCircuit, Vec<Diagnostic>>;
```

### 8.2 Índices internos

Usar newtypes para impedir mistura acidental de índices:

``` rust
pub struct PinIx(pub u32);
pub struct NetIx(pub u32);
pub struct SigIx(pub u32);
pub struct DeviceIx(pub u32);
```

Os índices são internos e podem ser densos. O documento persistido
continua utilizando IDs estáveis.

### 8.3 Representação compilada

``` rust
pub struct CompiledCircuit {
    pub pins: Vec<CompiledPin>,
    pub nets: Vec<CompiledNet>,
    pub switches: Vec<CompiledSwitch>,
    pub sources: Vec<CompiledSource>,
    pub loads: Vec<CompiledLoad>,
    pub signals: SignalTable,
    pub device_map: DeviceMap,
}
```

As estruturas definitivas serão refinadas durante o protótipo. O
contrato deve preservar a separação entre IDs do documento e índices de
execução.

### 8.4 Determinismo da compilação

Para que a ordem de componentes e fios não altere o resultado:

1.  Ordenar entidades por IDs estáveis antes de atribuir índices.
2.  Canonicalizar as listas de fontes e conexões.
3.  Não expor como identidade pública a raiz arbitrária do Union-Find.
4.  Ordenar diagnósticos e listas de falhas.
5.  Comparar resultados normalizados nos testes.

A garantia de determinismo bit a bit é apropriada para o motor lógico
inteiro/booleano. Um solver analógico futuro com ponto flutuante
precisará de critérios numéricos próprios.

------------------------------------------------------------------------

## 9. Solver elétrico discreto

### 9.1 Escopo

O solver do MVP trabalha com conectividade ideal e regras discretas. Ele
determina quais terminais pertencem à mesma rede condutiva, quais fontes
estão presentes e se há incompatibilidades configuradas.

Não calcula:

-   tensão instantânea;
-   corrente;
-   impedância;
-   queda de tensão;
-   divisão de tensão;
-   transitórios;
-   aquecimento físico.

### 9.2 Union-Find

O Union-Find é adequado para agrupar terminais unidos por fios e
contatos fechados, desde que o modelo seja de conectividade ideal.

``` rust
pub fn solve(
    circuit: &CompiledCircuit,
    signals: &DeviceSignals,
) -> ElectricalSolution;
```

O processo:

1.  Criar uma estrutura de conjuntos para os pinos.
2.  Unir terminais conectados por fios e junções.
3.  Unir terminais de switches fechados.
4.  Agrupar fontes por rede.
5.  Validar combinações incompatíveis.
6.  Avaliar regras discretas de energização das cargas.
7.  Produzir solução e falhas estruturadas.

### 9.3 Curto-circuito

Uma rede com fontes incompatíveis deve produzir uma falha identificável:

``` rust
pub struct ShortCircuit {
    pub net: NetIx,
    pub conflicting_sources: Vec<SourceId>,
    pub affected_pins: Vec<PinRef>,
}
```

O tratamento de L1-L2, L1-PE, polaridades DC e outras combinações deve
ser definido por regras explícitas do perfil de alimentação. Não se deve
inferir que qualquer diferença de identificador equivale automaticamente
a curto em todos os contextos.

### 9.4 Cargas

Uma carga pode possuir dois ou mais terminais. Para o MVP, a avaliação
será limitada a regras discretas definidas por tipo de carga e perfil de
alimentação.

Uma lâmpada de comando DC, por exemplo, pode ser considerada alimentada
quando seus terminais estão em redes compatíveis com a alimentação
configurada.

Um motor trifásico simplificado poderá ser marcado como alimentado
quando as três fases requeridas estiverem presentes e a sequência for
válida. Isso não equivale a calcular torque, corrente ou velocidade.

### 9.5 Nós flutuantes e cargas em série

O solver por conectividade ideal não resolve corretamente, de forma
geral, cargas em série, pontos estrela flutuantes ou divisões de tensão.

Não usar "potenciais virtuais" como solução genérica. Isso poderia
classificar redes inválidas como energizadas.

Estratégia:

-   MVP: declarar explicitamente os circuitos suportados e emitir
    diagnóstico para casos não suportados.
-   Etapa intermediária: análise de caminhos e ramos, sem alegar cálculo
    analógico.
-   Etapa avançada: implementar um solver resistivo, por exemplo MNA
    (Modified Nodal Analysis), com modelos de componentes e critérios de
    convergência.

A fronteira entre compilação, solução elétrica e lógica deve permitir
substituir ou ampliar o solver sem reescrever o editor.

------------------------------------------------------------------------

## 10. Motor lógico e semântica temporal

### 10.1 Modelo de execução

O runtime utiliza tempo virtual e ticks de duração configurada. Não lê o
relógio do sistema.

Para o MVP, recomenda-se um tick fixo configurado no `Simulator`, por
exemplo 1 ms, sujeito a validação de desempenho e semântica. A interface
gráfica pode avançar zero, um ou vários ticks conforme a política de
atualização, mas não deve alterar arbitrariamente a duração lógica de
cada tick.

### 10.2 Ordem de execução

Cada tick possui uma sequência definida:

1.  Capturar e aplicar entradas externas destinadas à fronteira temporal
    atual.
2.  Resolver a rede usando os estados efetivos atuais dos contatos.
3.  Avaliar bobinas, temporizadores e dispositivos a partir da solução
    atual.
4.  Construir snapshot correspondente à rede efetivamente resolvida.
5.  Confirmar os estados seguintes para o próximo tick.
6.  Avançar o tempo virtual.

O snapshot não pode misturar a solução calculada com os sinais futuros
já confirmados. O contrato deverá identificar claramente se o snapshot
representa o estado resolvido antes ou depois do commit. A recomendação
é representar o estado efetivamente resolvido naquele tick e publicar os
estados seguintes como efetivos no tick subsequente.

### 10.3 Estado

``` rust
pub struct RuntimeState {
    pub t_us: u64,
    pub current: DeviceSignals,
    pub next: DeviceSignals,
    pub timers: Vec<TimerState>,
}

pub struct DeviceSignals {
    pub bits: FixedBitSet,
}
```

A tabela de sinais é criada durante a compilação e associa cada sinal a
uma entidade estável. Botões, bobinas e saídas de temporizadores não
devem compartilhar índices por acidente; cada sinal deve ter origem,
tipo e política de atualização documentados.

### 10.4 Interface do simulador

``` rust
pub struct Simulator {
    tick_duration_us: u32,
    circuit: CompiledCircuit,
    state: RuntimeState,
}

impl Simulator {
    pub fn step(&mut self, inputs: &Inputs)
        -> Result<SimSnapshot, SimulationError>;
}
```

O tick fixo pertence à configuração do simulador. Para reprodução, devem
ser registrados o circuito compilado ou seu fingerprint, a configuração
temporal, o estado inicial e a sequência ordenada de entradas.

### 10.5 Atraso de atuação

O atraso de um tick é uma consequência do modelo síncrono: contatos
controlados por uma bobina passam a ser efetivos no ciclo seguinte.

Isso não representa necessariamente o tempo mecânico real de um
contator. Em uma etapa posterior, introduzir parâmetros explícitos de
pickup e dropout:

``` rust
pub struct ActuatorTiming {
    pub pickup_delay_us: u64,
    pub dropout_delay_us: u64,
}
```

A oscilação de um circuito com contato NF realimentando a própria bobina
deverá ser determinada por estados e tempos de atuação definidos, e não
pelo frame rate da GUI.

------------------------------------------------------------------------

## 11. Temporizadores e dispositivos

### 11.1 TON

-   Entrada desligada: saída desligada e temporização reiniciada.
-   Entrada ligada: inicia contagem.
-   Preset atingido: saída ligada.
-   Entrada desligada: saída desliga e reinicia.

### 11.2 TOF

-   Entrada ligada: saída liga imediatamente.
-   Entrada desligada: inicia temporização de desligamento.
-   Durante o preset: saída permanece ligada.
-   Preset atingido: saída desliga.
-   Nova entrada ligada: cancela a temporização de desligamento.

A implementação deverá especificar o comportamento exato no limite do
preset, inclusive quando uma entrada muda na mesma fronteira temporal em
que o temporizador atinge o valor configurado.

### 11.3 Estado do temporizador

``` rust
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

O modelo final poderá ser simplificado se os testes demonstrarem que
menos estados são suficientes. A semântica observável é mais importante
que a quantidade de variantes.

### 11.4 Matriz mínima de comportamento

  Dispositivo    Entrada/condição                      Resultado
  -------------- ------------------------------------- -------------------------------------------
  Botão NA       Pressionado                           Contato fechado
  Botão NF       Pressionado                           Contato aberto
  Bobina         Energizada                            Sinal do dispositivo ativo
  Contator       Bobina ativa                          Contatos associados atuam conforme modelo
  Relé térmico   Disparado                             Contato de proteção muda de estado
  TON            Preset atingido                       Saída ativa
  TOF            Entrada desligada e preset expirado   Saída desativa
  Lâmpada        Alimentação compatível                Indicador ativo

Cada comportamento deverá possuir testes automatizados.

------------------------------------------------------------------------

## 12. Snapshot e integração com a interface

O snapshot deve ser uma estrutura imutável que representa os resultados
de um tick.

``` rust
pub struct SimSnapshot {
    pub tick: u64,
    pub t_us: u64,
    pub energized_devices: Vec<DeviceId>,
    pub closed_symbols: Vec<SymbolId>,
    pub live_nets: Vec<NetState>,
    pub faults: Vec<SimulationFault>,
}
```

Evitar `HashMap` na saída se a ordem de serialização e comparação for
relevante. Usar vetores ordenados ou mapas ordenados.

O snapshot pode ser consumido por editor e renderer sem que esses crates
dependam da implementação interna do simulador. O tipo deverá residir em
`circuit-domain` ou em um crate neutro de contratos de runtime, conforme
a evolução do domínio.

------------------------------------------------------------------------

## 13. Diagnósticos

### 13.1 Categorias

  -----------------------------------------------------------------------
  Categoria               Exemplos                Momento
  ----------------------- ----------------------- -----------------------
  Estrutural              ID duplicado, terminal  Edição/compilação
                          inexistente             

  Semântica               Referência de           Compilação
                          dispositivo não         
                          resolvida               

  Elétrica                Curto, fonte            Execução
                          incompatível, falha de  
                          proteção                

  Limitação do modelo     Topologia não suportada Compilação/execução
                          pelo solver discreto    
  -----------------------------------------------------------------------

### 13.2 Estrutura

``` rust
pub enum Severity {
    Info,
    Warning,
    Error,
}

pub enum DiagnosticCode {
    DuplicateDeviceReference,
    UnknownDeviceReference,
    MissingRequiredTerminal,
    InvalidConnection,
    ShortCircuit,
    ProtectiveEarthFault,
    FloatingNet,
    UnsupportedCircuitModel,
}

pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub subject: Subject,
    pub related: Vec<Subject>,
    pub message: String,
}
```

Os códigos são estáveis e documentados. Mensagens podem ser localizadas
futuramente; a lógica não deve depender do texto da mensagem.

### 13.3 Terminais desconectados

Nem todo terminal desconectado é erro. A obrigatoriedade deve ser
definida no catálogo.

Exemplos:

-   Contato auxiliar não utilizado: normalmente permitido.
-   Bobina sem circuito: aviso ou erro conforme contexto.
-   Motor sem alimentação: aviso.
-   Terminal obrigatório ausente: erro.
-   Fonte sem carga: informação ou aviso.
-   Junção sem conexão: erro estrutural.

------------------------------------------------------------------------

## 14. Editor gráfico e renderização

### 14.1 UI

A aplicação desktop utilizará egui/eframe. O código de UI fica
exclusivamente em `apps/desktop`.

Regiões sugeridas:

-   Toolbar.
-   Biblioteca de componentes.
-   Canvas.
-   Inspector de propriedades.
-   Painel inferior de diagnósticos e eventos.
-   Controles de simulação.

### 14.2 Coordenadas

O documento usa unidades lógicas inteiras. O renderer converte para
coordenadas de tela por meio de uma câmera:

``` rust
pub struct Camera {
    pub offset: Vec2,
    pub zoom: f32,
}
```

Devem existir transformações inversas para hit-test e edição.

### 14.3 Cena neutra

``` rust
pub fn build_scene(
    schematic: &Schematic,
    view: &View,
    snapshot: Option<&SimSnapshot>,
) -> Scene;
```

A cena deve conter primitivas e metadados suficientes para desenho,
seleção, exportação e testes. Não deve depender de egui.

### 14.4 Desempenho

Criar benchmarks desde cedo com documentos de 100, 500 e 2.000
componentes, medindo:

-   tempo de renderização;
-   hit-test;
-   roteamento;
-   compilação da topologia;
-   simulação;
-   invalidação e reconstrução de cache.

Usar culling por bounding box e invalidação seletiva. Não introduzir
otimizações complexas sem medição.

### 14.5 Undo e redo

Usar Command Pattern:

``` rust
pub trait Command {
    fn apply(&mut self, schematic: &mut Schematic)
        -> Result<(), CommandError>;

    fn revert(&mut self, schematic: &mut Schematic)
        -> Result<(), CommandError>;
}
```

O histórico deve ser transacional. Uma operação inválida não pode deixar
o documento parcialmente alterado.

------------------------------------------------------------------------

## 15. Persistência e formato `.ocircuit`

### 15.1 Formato

O formato inicial será JSON, com extensão `.ocircuit`.

Requisitos:

-   `schema_version` obrigatório;
-   IDs estáveis;
-   ordem de serialização determinística;
-   migrações explícitas;
-   validação antes de carregar;
-   salvamento atômico na camada de aplicação;
-   preservação de campos desconhecidos quando compatível com a
    estratégia de schema.

Exemplo ilustrativo:

``` json
{
  "schema_version": 1,
  "project": {
    "name": "Partida Direta",
    "grid_unit": 10
  },
  "devices": [],
  "symbols": [],
  "junctions": [],
  "wires": [],
  "supplies": []
}
```

### 15.2 Responsabilidade de I/O

`circuit-project` converte estruturas em JSON e vice-versa, mas não
acessa diretamente `std::fs`.

A aplicação desktop ou CLI é responsável por abrir, gravar, escolher
caminhos e realizar salvamento atômico.

### 15.3 Migrações

Cada versão de schema deve ter migração testada. Não alterar
silenciosamente a semântica de terminais ou componentes. A versão do
modelo do componente pode ser distinta da versão geral do documento.

------------------------------------------------------------------------

## 16. CLI headless

O crate `apps/cli` permite executar e validar circuitos sem abrir a
interface gráfica.

Comandos propostos:

``` text
opencircuit validate projeto.ocircuit
opencircuit simulate projeto.ocircuit --scenario partida-direta.json
opencircuit test examples/
opencircuit export projeto.ocircuit --format svg
```

A CLI deverá produzir diagnósticos legíveis e, opcionalmente, saída
estruturada para integração com CI.

O CLI é importante para testes de regressão, automação e uso em
pipelines de documentação.

------------------------------------------------------------------------

## 17. Testes e qualidade

### 17.1 Testes unitários

-   catálogo de componentes;
-   resolução de terminais;
-   Union-Find;
-   contatos NA/NF;
-   bobinas;
-   TON e TOF;
-   serialização;
-   migrações;
-   comandos undo/redo;
-   transformações de coordenadas.

### 17.2 Golden tests

Cada exemplo deve conter:

-   documento JSON;
-   sequência de eventos de entrada;
-   configuração temporal;
-   trace esperado;
-   diagnósticos esperados.

Cenários:

**Partida direta** - S1 NA; - S2 NF; - K1 e contato de selo; - motor
como carga simplificada; - relé térmico.

**Reversor** - K1 e K2; - intertravamento elétrico; - sequência de fases
invertida no modelo de motor; - verificação de que não há acionamento
simultâneo proibido.

**Estrela-triângulo** - KM1 principal; - KM2 triângulo; - KM3 estrela; -
KT1 temporizador; - teste de ausência de sobreposição entre estrela e
triângulo. - Este cenário é inicialmente uma referência de requisitos;
sua execução funcional depende do modelo de enrolamentos e da capacidade
elétrica apropriada.

### 17.3 Testes de propriedades

Com `proptest`:

-   permutação de componentes e fios não altera resultados normalizados;
-   recompilar o mesmo documento gera a mesma representação canônica;
-   salvar e carregar preserva a semântica;
-   clone/checkpoint/restore e replay reproduzem o trace;
-   operações de conexão e desconexão não deixam referências inválidas.

### 17.4 Replay e rewind

`Clone` permite copiar um estado, mas não implementa rewind sozinho.

A estratégia deverá combinar:

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

Cada checkpoint deve estar associado ao fingerprint do circuito
compilado e à configuração temporal. Não restaurar um estado em uma
versão incompatível do circuito.

### 17.5 UI

Usar `egui_kittest` para fluxos essenciais:

-   abrir exemplo;
-   selecionar componente;
-   conectar terminais;
-   acionar botão;
-   observar estado do contator;
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
cargo xtask check-architecture
cargo check -p circuit-simulation --target wasm32-unknown-unknown
```

Quando a aplicação web estiver em desenvolvimento, adicionar build e
testes do alvo desktop WASM. Não assumir que a compilação do motor
garante a portabilidade da aplicação inteira.

------------------------------------------------------------------------

## 18. Exportação e símbolos

### 18.1 Exportação

O exportador recebe `Scene`, não estruturas internas de egui.

Formatos planejados:

-   SVG;
-   PDF;
-   posteriormente PNG ou formatos vetoriais adicionais, se necessário.

A exportação deve ser executável pela CLI.

### 18.2 Símbolos e normas

O projeto criará símbolos próprios seguindo convenções eletrotécnicas
documentadas. Não copiar arquivos gráficos de bibliotecas externas sem
verificar licença e autorização.

A IEC 60617 é uma referência importante para símbolos gráficos
eletrotécnicos. O uso de símbolos não deve ser descrito como
conformidade normativa certificada sem revisão técnica e jurídica
apropriada. Também devem ser consideradas as normas ABNT/NBR aplicáveis
ao público e às jurisdições atendidas.

Cada símbolo deve ter metadados de origem, revisão e referência
normativa, sem incorporar conteúdo protegido sem licença.

------------------------------------------------------------------------

## 19. Segurança, licenças e governança

### 19.1 Licença

Licença pretendida: dual MIT OR Apache-2.0.

Manter os arquivos de licença na raiz e incluir política para
contribuições e dependências.

Não incorporar código GPL ou outras licenças incompatíveis sem decisão
explícita sobre a licença e distribuição do projeto. A compatibilidade
deve ser analisada dependência por dependência.

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
-   todo componente deve declarar seus terminais;
-   mudanças de schema exigem migração;
-   mudanças de comportamento exigem regressão;
-   dependências Git exigem revisão e responsável;
-   recursos gráficos devem ter proveniência e licença registradas.

------------------------------------------------------------------------

## 20. Roadmap

### M0 --- Fundação e contratos

-   Workspace e toolchain.
-   IDs estáveis.
-   Separação Device/SymbolInstance.
-   Terminais estáveis.
-   Modelo de alimentação.
-   Semântica temporal.
-   Diagnósticos estruturais.
-   CI multiplataforma.
-   Verificador de arquitetura.

**Critério de conclusão:** contratos documentados, compilando e cobertos
por testes iniciais.

### M1 --- Motor lógico

-   Catálogo inicial.
-   Compilador de topologia.
-   Union-Find.
-   Solver discreto.
-   Botões, bobinas e contatos.
-   TON e TOF.
-   Partida direta.
-   Reversor.
-   CLI headless.
-   Golden tests.

**Critério de conclusão:** cenários de comando executados sem GUI, com
traces determinísticos.

### M2 --- Editor desktop

-   egui/eframe.
-   Canvas e câmera.
-   Biblioteca de componentes.
-   Seleção e movimentação.
-   Fios e junções.
-   Undo/redo.
-   JSON e migrações iniciais.
-   Snapshot e visualização da simulação.

**Critério de conclusão:** desenhar, salvar, abrir e simular uma partida
direta.

### M3 --- Circuitos avançados e exportação

-   Modelo de motor com seis terminais.
-   Enrolamentos.
-   Estrela-triângulo.
-   Avaliação de solver especializado ou MNA.
-   Diagnósticos elétricos ampliados.
-   Exportação SVG/PDF.

**Critério de conclusão:** cenário estrela-triângulo validado com modelo
elétrico adequado, sem sobreposição indevida dos contatores.

### M4 --- Ecossistema

-   Aplicação WASM.
-   Integração Modbus TCP por adaptador.
-   Avaliação de OPC UA e MQTT.
-   Plugins WASM.
-   Mais componentes e exemplos.
-   Documentação de extensão e comunidade.

**Critério de conclusão:** plataforma extensível, com interfaces de
integração documentadas e limites de segurança definidos.

------------------------------------------------------------------------

## 21. Riscos e mitigação

  -----------------------------------------------------------------------
  Risco                   Impacto                 Mitigação
  ----------------------- ----------------------- -----------------------
  Mistura de dispositivo  Alto                    Separar entidades antes
  e símbolo                                       do compilador

  Modelo discreto         Alto                    Documentar escopo e
  limitado                                        rejeitar casos não
                                                  suportados

  Semântica temporal      Alto                    Definir tick, commit e
  ambígua                                         snapshot formalmente

  Referências instáveis   Alto                    Chaves estáveis de
  de pinos                                        terminal

  Cruzamentos confundidos Alto                    Representar junções
  com junções                                     explicitamente

  Forks de terceiros      Médio/alto              Preferir crates.io e
                                                  fixar revisões somente
                                                  quando necessário

  egui em canvas CAD      Médio/alto              Benchmarks, culling e
  grande                                          cache

  Portabilidade WASM      Médio                   Núcleo sem I/O de
                                                  sistema e CI por alvo

  Símbolos e licenças     Médio                   Símbolos próprios e
                                                  proveniência
                                                  documentada

  Escopo excessivo        Alto                    MVP restrito e
                                                  critérios de saída por
                                                  marco

  Uso como validação de   Alto                    Avisos, documentação e
  segurança real                                  limites explícitos
  -----------------------------------------------------------------------

------------------------------------------------------------------------

## 22. Architectural Decision Records

ADRs recomendados antes de congelar os contratos:

  ADR       Decisão
  --------- ---------------------------------------------
  ADR-001   Rust e egui/eframe
  ADR-002   Separação Device, SymbolInstance e Terminal
  ADR-003   Identificadores estáveis de terminais
  ADR-004   Modelo de alimentação e condutores
  ADR-005   Semântica temporal do runtime
  ADR-006   Limites do solver discreto
  ADR-007   Formato JSON e versionamento
  ADR-008   Command Pattern para undo/redo
  ADR-009   Determinismo, checkpoint e replay
  ADR-010   Política de dependências, forks e licenças

Cada ADR deverá registrar contexto, decisão, alternativas consideradas e
consequências.

------------------------------------------------------------------------

## 23. Primeiro incremento executável

A primeira entrega de código deve ser deliberadamente pequena. Não criar
todos os crates e interfaces de uma só vez.

Crates iniciais:

1.  `circuit-domain`
2.  `circuit-components`
3.  `circuit-topology`
4.  `circuit-logic`
5.  `circuit-electrical`
6.  `circuit-simulation`
7.  `apps/cli`

A GUI pode começar em paralelo somente depois de o modelo de domínio
estar estável.

### Critério técnico

Construir um cenário headless que:

1.  carrega um circuito de partida direta;
2.  compila o documento;
3.  aplica evento de pressionar LIGA;
4.  executa ticks;
5.  verifica energização de K1;
6.  solta LIGA;
7.  verifica retenção pelo contato de selo;
8.  pressiona DESLIGA;
9.  verifica desenergização;
10. compara o trace com o golden esperado.

A GUI será considerada uma camada de apresentação desse mesmo cenário,
não uma implementação paralela da lógica.

------------------------------------------------------------------------

## 24. Decisões pendentes que exigem protótipo

Estas questões não devem ser resolvidas apenas por opinião arquitetural.
Precisam de protótipos e testes:

1.  **Modelo de rede:** qual é o menor modelo de condutores e fontes que
    atende aos circuitos de comando do MVP?
2.  **Carga trifásica:** quais propriedades do motor simplificado serão
    apresentadas sem sugerir cálculo físico?
3.  **Temporização:** qual tick fixo oferece uma boa combinação de
    resposta e desempenho?
4.  **Editor:** qual estratégia de roteamento automático é suficiente
    para a primeira versão?
5.  **Persistência:** como preservar campos desconhecidos e migrar
    mudanças no catálogo?
6.  **WASM:** quais dependências e recursos de UI funcionam nos
    navegadores-alvo?
7.  **Exportação:** qual biblioteca SVG/PDF oferece licença, qualidade e
    portabilidade adequadas?
8.  **Diagnósticos:** quais falhas são bloqueantes e quais são apenas
    avisos?

A resposta a essas perguntas deve ser registrada em ADRs ou atualizações
deste documento.

------------------------------------------------------------------------

## 25. Conclusão

A arquitetura proposta para o OpenCircuit é viável como projeto open
source em Rust, desde que o escopo do solver inicial seja descrito com
precisão e as fronteiras do domínio sejam implementadas antes de ampliar
o editor.

As decisões mais importantes da versão consolidada são:

-   manter Rust no núcleo e na UI;
-   utilizar egui/eframe para o desktop;
-   separar dispositivos, símbolos e terminais;
-   compilar o documento em uma representação executável;
-   manter a simulação headless, determinística e baseada em tempo
    virtual;
-   utilizar um solver discreto apenas para os casos que ele consegue
    representar corretamente;
-   preservar o formato de projeto por meio de IDs estáveis e migrações;
-   validar as camadas internas com ferramentas próprias;
-   evoluir para modelos elétricos mais completos somente quando os
    casos de uso exigirem.

**A prioridade não é desenhar muitos símbolos. É provar que um circuito
pequeno possui uma semântica correta, reproduzível e testável.**

O primeiro marco de sucesso é uma partida direta de motor que possa ser
criada, compilada, simulada, salva, reaberta e validada por testes
automatizados.

A partir desse núcleo, o OpenCircuit poderá crescer sem transformar a
interface gráfica em dona da lógica elétrica.

------------------------------------------------------------------------

## Apêndice A --- Checklist de aprovação da arquitetura

-   [ ] Device, SymbolInstance e Terminal separados.
-   [ ] IDs internos e chaves persistidas estáveis.
-   [ ] Junções explícitas e cruzamentos não ambíguos.
-   [ ] Condutores separados de grandezas elétricas.
-   [ ] Perfil de alimentação documentado.
-   [ ] Semântica do tick formalizada.
-   [ ] Snapshot temporalmente coerente.
-   [ ] Limites do solver discreto documentados.
-   [ ] Referências de contatos resolvidas pelo compilador.
-   [ ] Diagnósticos estruturais, semânticos e de runtime separados.
-   [ ] Regras de dependência verificadas pelo CI.
-   [ ] Formato JSON versionado e com migrações.
-   [ ] CLI headless funcional.
-   [ ] Golden test de partida direta aprovado.
-   [ ] Política de licenças e recursos gráficos documentada.

## Apêndice B --- Referências técnicas

As versões e APIs devem ser verificadas novamente durante a
implementação. Referências úteis:

-   Rust: https://www.rust-lang.org/
-   Cargo Book: https://doc.rust-lang.org/cargo/
-   egui: https://docs.rs/egui/
-   eframe: https://docs.rs/eframe/
-   serde: https://serde.rs/
-   petgraph: https://docs.rs/petgraph/
-   fixedbitset: https://docs.rs/fixedbitset/
-   cargo-deny: https://embarkstudios.github.io/cargo-deny/
-   proptest: https://docs.rs/proptest/
-   insta: https://insta.rs/
-   Criterion: https://bheisler.github.io/criterion.rs/book/
-   IEC 60617: https://std.iec.ch/iec60617

------------------------------------------------------------------------

**Fim do documento --- OpenCircuit Arquitetura Técnica v0.2**
