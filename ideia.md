# **Análise técnica do OpenCircuit v0.1**

Li o documento como se estivesse revisando uma proposta de arquitetura para aprovação antes do início da implementação.

Minha avaliação geral é positiva: **o documento está muito mais próximo de uma especificação de engenharia de software do que de uma simples ideia de projeto.** Há boas decisões de separação de responsabilidades, determinismo, persistência, testes e independência da interface gráfica.

Mas existem alguns problemas importantes que eu corrigiria antes de considerar essa arquitetura aprovada.

O principal deles é a modelagem elétrica: o documento ainda mistura três conceitos que precisam ser rigorosamente separados:

1. Continuidade elétrica: quais terminais estão conectados por fios ou contatos fechados.
2. Potencial elétrico: qual tensão ou potencial existe em determinado nó.
3. Comportamento funcional: quais componentes estão energizados e quais sinais produzem.

Essa distinção afeta o solver, a detecção de curtos, os motores trifásicos, os temporizadores e a fidelidade da simulação.

## **1. Parecer executivo**

**Avaliação da arquitetura**

Promissora

# **8,2/10**

Nota técnica subjetiva para a proposta v0.1, não para um software implementado.

| Critério                       | Nota |
| ------------------------------ | ---- |
| Separação de responsabilidades | 9,5  |
| Modularidade do workspace      | 9,0  |
| Determinismo e testabilidade   | 9,0  |
| Modelagem do domínio           | 8,0  |
| Motor de simulação             | 6,5  |
| Modelagem elétrica             | 6,0  |
| Persistência                   | 8,5  |
| Estratégia multiplataforma     | 8,5  |
| Plano de testes                | 9,0  |
| Estratégia open source         | 8,5  |

**Veredito:** aprovaria a direção arquitetural, mas não congelaria ainda os contratos de `circuit-topology`, `circuit-electrical`, `circuit-logic` e `circuit-simulation`.

A boa notícia é que não vejo necessidade de abandonar Rust, egui, a arquitetura por crates ou o formato JSON.

Vejo necessidade de melhorar principalmente a semântica da simulação.

Vou separar os problemas em quatro categorias:

- **Crítico:** pode produzir resultados elétricos incorretos.
- **Alto:** pode gerar limitações arquiteturais difíceis de corrigir.
- **Médio:** merece correção antes da implementação ou durante o primeiro incremento.
- **Baixo:** melhoria de qualidade, manutenção ou experiência de desenvolvimento.

## **2. Problemas críticos que precisam ser resolvidos**

### **2.1.&#x20;**`Tag`**&#x20;duplicada: existe uma contradição no modelo**

Crítico

Este é um dos primeiros problemas que corrigiria.

Você definiu:

```
pub struct Tag(pub String);

pub enum Kind {
    Contact {
        form: Form,
        driver: Tag,
    },
    Coil,
}
```

E também exige diagnóstico de tag duplicada.

Imagine um contator K1:

| Elemento             | Referência |
| -------------------- | ---------- |
| Bobina               | K1         |
| Contato NA auxiliar  | K1         |
| Contato NF auxiliar  | K1         |
| Contatos de potência | K1         |

É perfeitamente legítimo que vários símbolos pertençam ao mesmo dispositivo e compartilhem a identificação K1.

Portanto, duplicidade de tag não pode simplesmente ser considerada erro.

**Minha proposta: separar dispositivo físico de símbolo gráfico.**

```
pub struct DeviceId(pub u64);
pub struct SymbolId(pub u64);

pub struct ReferenceDesignator(pub String);

pub struct Device {
    pub id: DeviceId,
    pub reference: ReferenceDesignator,
    pub kind: DeviceKind,
}

pub struct SymbolInstance {
    pub id: SymbolId,
    pub device_id: DeviceId,
    pub kind: SymbolKind,
}
```

Exemplo:

```
Device K1
│
├── Symbol: Coil K1
├── Symbol: Contact NO K1
├── Symbol: Contact NC K1
├── Symbol: Main Contact 1 K1
└── Symbol: Main Contact 2 K1
```

Agora temos:

- Um dispositivo físico.
- Várias representações gráficas.
- Uma identidade interna.
- Uma referência visível.
- Um estado lógico associado ao dispositivo.

O contato não precisa procurar a bobina por uma String. Ele conhece o `DeviceId`.

A resolução de tags passa a ser responsabilidade do compilador do circuito, que valida se existem referências duplicadas entre dispositivos distintos.

**Impacto:** alto. Essa alteração deverá acontecer antes de implementar `SignalTable`.

### **2.2.&#x20;**`Potential`**&#x20;está representando coisas diferentes**

Crítico

Seu enum:

```
pub enum Potential {
    L1,
    L2,
    L3,
    N,
    PE,
    Dc24Pos,
    Dc0V,
}
```

Não representa propriamente potenciais elétricos. Ele representa identificações de condutores ou referências de alimentação.

Isso importa por três razões.

**Primeiro: fases não são tensões absolutas.**

L1, L2 e L3 identificam condutores de um sistema trifásico. Para calcular tensões, precisamos também de parâmetros da alimentação:

- Tensão nominal.
- Frequência.
- Sequência de fases.
- Referência do sistema.
- Relação entre tensão fase-neutro e fase-fase.

**Segundo: PE não é uma fase nem uma fonte de alimentação.**

O PE é um condutor de proteção. Ele não deve ser considerado uma fonte que energiza uma lâmpada ou um motor.

Uma ligação acidental entre L1 e PE deve ser tratada como falha elétrica, e não como uma alimentação válida.

**Terceiro: N e PE não são equivalentes.**

Uma eventual ligação entre neutro e proteção depende da configuração do sistema de alimentação e do ponto de equipotencialização. Não se deve simplesmente tratar os dois como o mesmo nó em qualquer circunstância.

### **Minha proposta**

Renomearia `Potential` para `Conductor`.

```
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

E adicionaria uma configuração de alimentação:

```
pub struct SupplyProfile {
    pub id: SupplyId,
    pub kind: SupplyKind,
    pub nominal_voltage_mv: u32,
    pub frequency_millihz: Option<u32>,
}

pub enum SupplyKind {
    SinglePhase,
    ThreePhase,
    Dc,
}
```

Esse código é uma base conceitual. A sequência de fases, as relações entre condutores, os tipos de carga e os valores nominais também precisarão fazer parte da definição completa da alimentação.

Para o MVP, podemos continuar com um modelo discreto, sem resolver equações de tensão e corrente.

Mas precisamos deixar explícito:

> O motor determina compatibilidade entre condutores e condições de energização. Ele não calcula grandezas elétricas analógicas.

Isso evita prometer uma precisão física que ainda não existe.

### **2.3. O ciclo de scan possui uma ambiguidade temporal**

Crítico

Você escreveu:

```
s.apply_inputs(inp);

let sol = electrical::solve(nl, &s.signals);

logic::step(
    nl,
    s,
    &sol.energized,
    dt_us,
);

s.t_us += dt_us as u64;

snapshot(nl, s, &sol)
```

A ideia é boa, mas há um problema de consistência.

Observe:

1. `electrical::solve` usa os sinais anteriores.
2. `logic::step` modifica os sinais.
3. `snapshot` recebe o novo estado e a solução anterior.

Se `snapshot` calcula os contatos fechados usando `s.signals`, ele pode informar que K1 já fechou seus contatos, embora a solução elétrica tenha sido calculada quando esses contatos ainda estavam abertos.

Isso mistura dois instantes distintos.

Não basta dizer que o contato muda no tick seguinte. É necessário definir exatamente o que o snapshot representa.

### **Minha correção: estado atual e estado seguinte**

Separaria o processo em estágios:

**1. Sample Inputs**

Capturar os eventos externos do ciclo.

**2. Solve Current Network**

Resolver utilizando os contatos e estados efetivos atuais.

**3. Evaluate Devices**

Calcular bobinas, temporizadores e saídas futuras.

**4. Build Snapshot**

Registrar os resultados da rede efetivamente resolvida.

**5. Commit Next State**

Tornar os novos estados disponíveis para o ciclo seguinte.

O contrato poderia ficar assim:

```
pub struct RuntimeState {
    pub current: DeviceSignals,
    pub next: DeviceSignals,
    pub timers: Vec<TimerState>,
    pub t_us: u64,
}
```

E:

```
pub fn tick(
    circuit: &CompiledCircuit,
    state: &mut RuntimeState,
    inputs: &Inputs,
    dt_us: u32,
) -> Result<SimSnapshot, SimulationError>;
```

A implementação deve garantir que o snapshot represente uma única fronteira temporal coerente.

Outra questão: seu `dt_us` é passado a cada chamada. Isso permite que o chamador varie o tempo entre ticks, apesar de a documentação afirmar que a simulação possui tick fixo.

Eu escolheria uma destas alternativas:

- `Simulator` possui um `tick_duration_us` fixo e o usuário chama `step()`.
- O motor aceita `dt_us`, mas a sequência de passos temporais também integra os dados necessários para reproduzir a execução.

Para seu caso, prefiro a primeira.

### **Sobre o atraso de um tick**

Há uma distinção importante:

**Atraso de scan não é atraso mecânico.**

Um contator real possui tempos de atuação e liberação. Se utilizarmos um tick de 1 ms, ele terá uma atuação artificialmente mais rápida do que se utilizarmos 20 ms.

Para o MVP, tudo bem ter um modelo lógico síncrono, desde que isso seja documentado.

Mais tarde, introduza parâmetros:

```
pub struct ActuatorTiming {
    pub pickup_delay_us: u64,
    pub dropout_delay_us: u64,
}
```

Isso permitirá modelar o comportamento de contatores e relés com maior fidelidade.

O buzzer de relé com contato NF realimentando sua bobina pode oscilar, mas a frequência deve depender dos tempos de atuação e liberação, não simplesmente do intervalo de atualização da tela.

### **2.4. A propagação virtual de potencial não deve ser a solução definitiva**

Crítico

Você identificou corretamente que o algoritmo atual não resolve:

- Lâmpadas em série.
- Resistores em série.
- Ponto estrela de motores.
- Nós flutuantes.
- Divisores de tensão.

Mas sua alternativa A:

> Um net sem fonte, ligado a duas cargas cujos outros terminais possuem potenciais diferentes, recebe potencial virtual.

Tem um problema: isso cria uma interpretação elétrica sem resolver a rede como um todo.

Em uma rede com ramificações, cargas paralelas, ligações adicionais e componentes com múltiplos terminais, esse algoritmo pode classificar incorretamente os nós e cargas.

Não recomendo utilizar potenciais virtuais como mecanismo geral.

### **O que eu faria?**

Implementaria três níveis de capacidade:

| Nível | Capacidade                           | Técnica                                   |
| ----- | ------------------------------------ | ----------------------------------------- |
| 1     | Comandos industriais simples         | Propagação discreta                       |
| 2     | Redes com caminhos através de cargas | Grafo de ramos e análise de conectividade |
| 3     | Tensões e correntes reais            | MNA                                       |

O primeiro nível é suficiente para o MVP.

O segundo poderá resolver questões de continuidade e existência de caminhos de corrente sem ainda calcular valores de tensão.

O terceiro será necessário quando você quiser calcular grandezas elétricas, divisões de tensão e comportamentos de componentes passivos.

**Não tente fazer o nível 1 parecer um simulador analógico de nível 3.**

### **Uma questão específica sobre o motor trifásico**

Seu `Motor3PhWindings` com seis terminais é interessante, mas precisa ter uma representação interna de três enrolamentos:

```
U1 ───── Winding U ───── U2
V1 ───── Winding V ───── V2
W1 ───── Winding W ───── W2
```

Esses enrolamentos são ramos, não conexões ideais.

No arranque estrela-triângulo, o sistema precisa representar o ponto estrela flutuante e as alterações nas ligações dos enrolamentos.

O MNA é uma solução natural para o estágio físico. Uma solução especializada baseada em conectividade pode ser suficiente para o estado binário de energização, mas precisa ser testada contra configurações de referência.

Minha decisão seria retirar `Motor3PhWindings` do MVP e introduzir esse componente no marco de estrela-triângulo.

Assim não comprometemos a arquitetura inicial.

### **2.5. Você está misturando símbolo, dispositivo e componente**

Crítico

Esse problema está relacionado às tags, mas merece uma decisão própria.

Atualmente:

```
pub struct Component {
    pub id: ComponentId,
    pub tag: Tag,
    pub kind: Kind,
    pub pos: (i32, i32),
    pub rot: u8,
}
```

O componente desempenha três papéis:

- Representação visual.
- Elemento do circuito.
- Unidade de comportamento lógico.

Isso funciona no início, mas fica difícil quando você introduzir contatores com múltiplos contatos, temporizadores com várias saídas e componentes que possuem diversas representações.

Eu adotaria três entidades:

| Entidade       | Responsabilidade             |
| -------------- | ---------------------------- |
| Device         | Equipamento físico ou lógico |
| SymbolInstance | Representação gráfica        |
| Terminal       | Ponto de conexão             |

O circuito passa a ser um documento que contém símbolos, dispositivos e conexões.

Uma bobina e seus contatos auxiliares são símbolos diferentes que apontam para o mesmo dispositivo.

Isso também permite que o dispositivo tenha configurações de atuação independentes da representação gráfica.

## **3. Revisão do modelo de dados**

Agora vou propor uma evolução sem transformar seu projeto em um framework gigantesco.

### **3.1. Identidade dos pinos**

Você utiliza:

```
pub struct Pin {
    pub comp: ComponentId,
    pub idx: u8,
}
```

É simples e eficiente, mas existe um risco de compatibilidade.

Imagine que a versão 1 do componente tenha:

```
0 = A1
1 = A2
2 = 13
3 = 14
```

E a versão 2 altere a ordem dos terminais.

Se os fios armazenam apenas índices, um documento antigo poderá conectar os fios ao terminal errado.

Minha sugestão é utilizar identificadores estáveis para os terminais:

```
pub struct TerminalKey(pub String);

pub struct PinRef {
    pub symbol_id: SymbolId,
    pub terminal: TerminalKey,
}
```

Exemplos de identificadores:

```
A1
A2
13
14
21
22
U1
V1
W1
```

O índice numérico pode continuar existindo internamente no `Netlist`, porque é eficiente para os algoritmos.

Mas o arquivo de projeto deve armazenar uma referência estável.

### **3.2. Geometria dos fios**

Seu `Wire`:

```
pub struct Wire {
    pub a: Pin,
    pub b: Pin,
}
```

Representa conectividade, mas não representa suficientemente um desenho CAD.

Você precisará de:

- Pontos de passagem.
- Junções.
- Cruzamentos.
- Roteamento.
- Identificação de segmentos.

Eu acrescentaria entidades de junção:

```
pub struct Junction {
    pub id: JunctionId,
    pub position: GridPoint,
}
```

E permitiria que fios terminassem em terminais ou junções:

```
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

A geometria do fio continua separada da topologia elétrica.

Isso é importante para que dois fios que apenas se cruzam não sejam interpretados como ligados.

### **3.3. Rotação e posição**

Troque:

```
pub rot: u8
```

Por algo mais explícito:

```
pub enum QuarterTurn {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}
```

E:

```
pub struct GridPoint {
    pub x: i32,
    pub y: i32,
}
```

Isso elimina valores inválidos de rotação e padroniza as coordenadas.

### **3.4. O compilador do circuito precisa ter identidade própria**

Sua ideia de transformar `Schematic` em `Netlist` é excelente.

Eu deixaria esse processo formalizado:

```
pub fn compile(
    schematic: &Schematic,
    catalog: &ComponentCatalog,
) -> Result<CompiledCircuit, Vec<Diagnostic>>;
```

O resultado deverá conter:

- Índices densos de pinos.
- Índices de sinais.
- Mapeamento de pinos para IDs estáveis.
- Identificadores dos dispositivos.
- Referências de fontes.
- Ramos de carga.
- Switches controlados.
- Diagnósticos de compilação.

Uma atenção especial: `PinIx` e `SigIx` deveriam ser newtypes, e não aliases:

```
pub struct PinIx(pub u32);
pub struct SigIx(pub u32);
```

Assim evitamos passar um índice de sinal para uma função que espera um índice de pino.

### **3.5. Determinismo independente da ordem**

Sua exigência de que permutar fios e componentes não altere os resultados é excelente.

Mas há uma sutileza.

Se você atribuir índices densos conforme a ordem em que encontrar os componentes, a permutação dos elementos pode alterar os índices e, consequentemente, a saída bruta do `Solution`.

Para obter determinismo real:

1. Ordenar entidades por identificadores estáveis durante a compilação.
2. Criar índices densos a partir dessa ordem.
3. Ordenar as listas de diagnósticos e curtos.
4. Canonicalizar os conjuntos de fontes associados a cada net.
5. Comparar resultados normalizados nos testes.

A escolha do representante interno do Union-Find também não pode determinar a identidade pública de um nó.

E faria uma distinção na garantia de determinismo:

- Simulação lógica com estados booleanos e relógio inteiro: determinismo exato.
- Simulação analógica futura com ponto flutuante: definir tolerâncias e garantias numéricas por plataforma e implementação.

Não prometeria resultados bit a bit idênticos entre todas as arquiteturas para um futuro solver numérico.

## **4. Revisão dos contratos entre crates**

A organização geral está muito boa. Eu manteria a maioria das dependências propostas.

Há, porém, três ajustes.

### **4.1. O motor de simulação precisa conhecer o compilador**

Atualmente, o diagrama de dependências deixa `simulation` dependendo de `electrical` e `logic`, mas a simulação também precisa consumir o `Netlist` ou `CompiledCircuit` criado por `topology`.

Eu explicitamente declararia:

```
simulation
  ├── circuit-domain
  ├── circuit-topology
  ├── circuit-electrical
  ├── circuit-logic
  └── circuit-diagnostics
```

### **4.2. O crate de I/O não deve contaminar o núcleo**

Sua ideia:

```
circuit-io → simulation
```

É aceitável como adaptador, desde que o núcleo não precise conhecer Modbus, MQTT ou OPC UA.

A fronteira ideal seria:

```
Modbus Adapter
     |
     v
Input / Output Interface
     |
     v
Simulation Runtime
```

O adaptador envia entradas e recebe saídas.

O simulador não abre sockets nem consulta dispositivos de rede.

Isso será essencial para testes determinísticos e para a futura versão WASM.

### **4.3.&#x20;**`cargo-deny`**&#x20;não valida a arquitetura interna**

Correção necessária

Você escreveu que o grafo de dependências deve ser validado com `cargo tree` ou `cargo-deny`.

Aqui há uma pequena confusão de ferramentas.

- `cargo tree` permite inspecionar e visualizar dependências.
- `cargo-deny` verifica políticas de dependências, fontes, licenças, vulnerabilidades e restrições de pacotes.
- Nenhum deles, sozinho, implementa as regras específicas de camadas do seu workspace.

A própria documentação do cargo-deny descreve controles de fontes Git, licenças e advisories, não regras arbitrárias entre crates locais.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://embarkstudios.github.io\&sz=32)

cargo-deny+1



Eu criaria um verificador no CI:

```
xtask
  └── check-architecture
        ├── desktop → pode depender de todos
        ├── editor → não pode depender de simulation
        ├── renderer → não pode depender de egui
        ├── logic → não pode depender de editor
        └── domain → não pode depender de workspace
```

Esse programa pode usar `cargo metadata --format-version 1` para obter o grafo e comparar com uma lista de dependências proibidas.

É inclusive uma prática que combina com a referência CADCraft: seu repositório declara verificações próprias de camadas, além de formatação, testes e compilação WASM por meio de `cargo xtask ci`.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub



## **5. Avaliação da referência CADCraft**

Fiz uma verificação da referência pública, e há uma boa notícia: o bloco de patches que você incluiu corresponde ao utilizado no `Cargo.toml` atual do projeto CADCraft.

Conferi especificamente:

- A revisão do fork `egui`.
- A revisão do fork `winit`.
- Os patches de `krilla`, `hayro-syntax` e `hayro-jbig2`.
- As versões de `egui` e `eframe`.

A correspondência está presente no manifesto consultado em 13/11/2026.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub



Também confirmei que o CADCraft atualmente usa:

| Configuração        | CADCraft          |
| ------------------- | ----------------- |
| Edition             | 2024              |
| Resolver            | 3                 |
| Rust MSRV declarado | 1.95              |
| egui                | 0.37              |
| eframe              | 0.37              |
| Licença             | MIT OR Apache-2.0 |

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub

Isso corrige a observação de que as revisões ainda não tinham sido verificadas.

### **Minha recomendação sobre os forks**

Apesar de terem sido verificados, eu não os adotaria automaticamente.

O CADCraft é uma excelente referência para o editor e para a separação entre domínio, engine, renderização e UI. Seu README inclusive destaca que as camadas inferiores a `ui-egui` não dependem da interface egui.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub



Mas ele também possui necessidades específicas de CAD que não necessariamente existem no OpenCircuit.

Eu seguiria esta política:

Decisão recomendada

**MVP: crates.io, sem forks.**

Adotar forks somente quando:

1. Existir um problema reproduzível.
2. O upstream não oferecer solução adequada.
3. O fork corrigir efetivamente esse problema.
4. Houver testes comprovando a necessidade.
5. O custo de manutenção estiver documentado.

Isso reduz bastante a superfície de manutenção.

### **Edition 2021 ou 2024?**

Eu mudaria sua proposta para:

```
[workspace]
resolver = "3"

[workspace.package]
edition = "2024"
```

Desde que você aceite o MSRV necessário.

O resolver 3 foi introduzido para considerar a compatibilidade de versões do Rust na resolução de dependências, e é o comportamento padrão da Edition 2024.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://doc.rust-lang.org\&sz=32)

The Rust Edition Guide+1



Não existe necessidade de permanecer na Edition 2021 apenas porque um fork específico utiliza essa edição.

## **6. Diagnósticos: separar compilação de execução**

Essa separação ainda não está suficientemente explícita.

Você tem três tipos de diagnóstico que devem ter comportamentos diferentes.

| Categoria  | Exemplo                       | Momento           |
| ---------- | ----------------------------- | ----------------- |
| Estrutural | Pino obrigatório desconectado | Edição/compilação |
| Semântico  | Referência K1 inexistente     | Compilação        |
| Elétrico   | L1 conectado diretamente a L2 | Simulação         |

Eu criaria códigos estruturados:

```
pub enum DiagnosticCode {
    MissingRequiredTerminal,
    DuplicateDeviceReference,
    UnknownDeviceReference,
    InvalidConnection,
    ShortCircuit,
    ProtectiveEarthFault,
    FloatingNet,
    NonConvergentSimulation,
}
```

E um diagnóstico com identificação dos elementos envolvidos:

```
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub subject: Subject,
    pub related: Vec<Subject>,
    pub message: String,
}
```

A vantagem é que a interface pode destacar todos os componentes relacionados a um erro.

### **Atenção ao pino solto**

Não considero correto tratar qualquer terminal desconectado como erro.

Um contato auxiliar NF não utilizado pode perfeitamente estar sem ligação.

A validação deve considerar os requisitos de cada terminal e dispositivo.

Exemplo:

- Bobina sem circuito: aviso ou erro, conforme o contexto.
- Terminal não utilizado de contato auxiliar: normalmente permitido.
- Fonte sem destino: talvez apenas um aviso.
- Motor sem alimentação: aviso.
- Junção sem conexão: erro estrutural.

### **Atenção ao curto-circuito**

Eu não colocaria somente os pinos em `shorts`.

Usaria um objeto de falha:

```
pub struct ShortCircuit {
    pub net_id: NetId,
    pub conflicting_sources: Vec<SourceId>,
    pub affected_pins: Vec<PinRef>,
}
```

Isso possibilita mostrar o caminho da falha, o que é muito mais útil para o aprendizado e a depuração.

Também separaria diagnósticos estáticos dos estados de falha que surgem durante a simulação.

## **7. Temporizadores e dispositivos industriais**

O documento define:

```
pub enum TimerMode {
    OnDelay,
    OffDelay,
}
```

Isso é bom, mas ainda precisamos formalizar o comportamento.

### **TON — retardo na energização**

| Condição                         | Saída                |
| -------------------------------- | -------------------- |
| Entrada desligada                | Desligada            |
| Entrada ligada, antes do preset  | Desligada            |
| Entrada ligada, atingiu o preset | Ligada               |
| Entrada desligada novamente      | Desligada e reinicia |

### **TOF — retardo na desenergização**

| Condição                       | Saída                            |
| ------------------------------ | -------------------------------- |
| Entrada desligada inicialmente | Desligada                        |
| Entrada ligada                 | Ligada imediatamente             |
| Entrada desligada              | Continua ligada durante o preset |
| Preset expirado                | Desligada                        |

A implementação precisa definir precisamente o que acontece quando a entrada muda exatamente no limite do preset.

Também precisa ter uma estratégia para `dt` maior que o tempo restante: o temporizador não pode ultrapassar o preset nem depender de uma contagem baseada no relógio da interface.

Eu utilizaria uma máquina de estados:

```
pub enum TimerPhase {
    Idle,
    Timing,
    Active,
    Retiming,
}
```

Com um estado:

```
pub struct TimerState {
    pub phase: TimerPhase,
    pub elapsed_us: u64,
    pub output: bool,
    pub previous_input: bool,
}
```

Isso torna explícitos os estados necessários para TON e TOF.

Para os dispositivos do MVP, eu também criaria uma matriz de comportamento:

| Dispositivo      | Estado de entrada | Saída                       |
| ---------------- | ----------------- | --------------------------- |
| Botão NA         | Pressionado       | Contato fechado             |
| Botão NF         | Pressionado       | Contato aberto              |
| Bobina           | Energizada        | Sinais associados ativos    |
| Contator         | Bobina energizada | Contatos conforme modelo    |
| Relé térmico     | Disparado         | Contato de proteção aberto  |
| Temporizador TON | Tempo atingido    | Contato temporizado fechado |

Isso deve virar documentação executável: cada linha precisa ter pelo menos um teste automatizado.

## **8. Interface gráfica e estratégia CAD**

Sua decisão por egui é coerente.

O eframe 0.37 oferece suporte para aplicações nativas e web. A documentação apresenta `run_native` para desktop e `WebRunner` para WebAssembly.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://docs.rs\&sz=32)

Rust



Eu manteria sua arquitetura:

```
Schematic
    |
    v
Renderer
    |
    v
Scene
    |
    ├── egui
    ├── SVG
    └── PDF
```

Isso é uma boa decisão.

### **Três cuidados adicionais**

**1. Não armazene apenas primitivas de desenho.**

Você precisará guardar metadados dos símbolos:

- Identificador do símbolo.
- Terminais locais.
- Posições dos terminais.
- Pontos de referência.
- Texto e identificadores.
- Caixa delimitadora.

**2. Faça hit-testing no sistema de coordenadas do modelo.**

Não dependa diretamente de coordenadas de pixels. A câmera transforma coordenadas entre mundo e tela.

**3. Faça testes de desempenho cedo.**

Não espere ter 500 componentes para descobrir que o canvas apresenta problemas.

Eu criaria cenários com 100, 500 e 2.000 componentes, medindo:

- Renderização.
- Tempo de hit-test.
- Tempo de roteamento.
- Recompilação da topologia.
- Tempo de simulação.

Use culling por bounding box e invalidação seletiva da geometria.

O editor também deve usar caches de renderização sem armazenar informações que se tornem incompatíveis com o estado atual do circuito.

### **Renderização e exportação**

Uma boa decisão é fazer o PDF depender do SVG ou de uma cena gráfica neutra, e não da interface egui.

O exportador deve conseguir gerar o desenho sem abrir a aplicação.

Isso também torna o `apps/cli` útil para geração automatizada de documentação elétrica.

## **9. Cargo, dependências e compilação multiplataforma**

Algumas observações adicionais à sua seção 8.

### `fixedbitset`

É adequado para o seu conjunto de sinais booleanos. A versão documentada mais recente que consultei é 0.5.9, e sua implementação oferece também otimizações SIMD para alvos compatíveis, incluindo suporte a WASM SIMD.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://docs.rs\&sz=32)

Rust



Eu manteria essa biblioteca no motor lógico.

Mas não utilizaria o bitset para representar o estado elétrico de cada net. Uma net precisa de informações mais ricas do que verdadeiro ou falso.

### **WASM**

A afirmação de que WASM não possui relógio, threads ou arquivos deve ser refinada.

O problema é não possuir diretamente os mesmos serviços nativos de um sistema operacional.

O navegador pode oferecer serviços equivalentes por meio de APIs específicas. Arquivos, tempo, rede e paralelismo possuem restrições e mecanismos próprios.

Minha recomendação:

- Núcleo sem dependências de sistema operacional.
- Persistência sem acesso direto ao filesystem.
- Relógio virtual no simulador.
- Adaptadores específicos para desktop e navegador.
- Exportação e integração de arquivos testadas separadamente em cada alvo.

E não deixe a CI testar somente `circuit-simulation`.

Quando a aplicação estiver criada, teste também o próprio desktop no alvo WASM.

### **Cargo e edição**

Minha configuração sugerida:

```
[workspace]
resolver = "3"
members = [
    "apps/*",
    "crates/*",
]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.95"
```

O Rust 1.95 aqui é uma referência para alinhar com a configuração atual do CADCraft, não uma exigência que eu tenha demonstrado para cada dependência individual.

A versão mínima real deve ser determinada pelo conjunto de dependências do OpenCircuit e dos alvos que você decidir suportar.

### **Dependências adicionais que eu acrescentaria**

| Crate          | Uso                           |
| -------------- | ----------------------------- |
| `proptest`     | Testes de propriedades        |
| `insta`        | Snapshots e golden tests      |
| `tempfile`     | Testes de arquivos            |
| `criterion`    | Benchmarks                    |
| `clap`         | CLI                           |
| `egui_kittest` | Testes de interface           |
| `camino`       | Caminhos UTF-8, se necessário |

Não instalaria todas no início. `proptest` e `tempfile` seriam prioritárias.

## **10. Licenças e símbolos IEC**

Sua decisão de criar símbolos próprios é correta.

Há, porém, um alerta.

A IEC mantém a base oficial IEC 60617, que em sua edição de 2026 disponibiliza mais de 1.500 símbolos gráficos para diagramas eletrotécnicos, em diferentes formatos. Seu acesso é comercial, por assinatura.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://webstore.iec.ch\&sz=32)

IEC



Portanto:

- Não copie arquivos SVG, EPS, GIF ou DWG de bases de terceiros sem autorização.
- Não utilize bibliotecas de símbolos com licença indefinida.
- Registre a origem de cada recurso gráfico.
- Documente a convenção e a referência normativa utilizada.
- Valide cada símbolo antes de afirmar conformidade com uma norma.

Eu criaria um catálogo versionado:

```
assets/symbols/
├── contacts/
├── coils/
├── motors/
├── protection/
├── power/
└── signaling/
```

E uma especificação para cada símbolo:

```
pub struct SymbolDefinition {
    pub id: SymbolDefinitionId,
    pub terminals: Vec<SymbolTerminal>,
    pub geometry: SymbolGeometry,
    pub standard_reference: Option<String>,
}
```

A referência normativa não precisa incluir conteúdo protegido da norma; serve para rastrear a convenção utilizada.

## **11. Testes: o que está faltando**

Sua estratégia de testes é um dos pontos mais fortes do documento.

Eu adicionaria alguns testes obrigatórios antes da primeira versão pública.

### **Matriz de testes elétricos e lógicos**

| Cenário                             | Resultado esperado                          |
| ----------------------------------- | ------------------------------------------- |
| Botão NA pressionado                | Fecha contato                               |
| Botão NF pressionado                | Abre contato                                |
| Contator energizado                 | Ativa contatos associados                   |
| Bobina sem alimentação              | Contatos retornam ao repouso                |
| Circuito com selo                   | Mantém contator após soltar LIGA            |
| Parada acionada                     | Desenergiza contator                        |
| Relé térmico disparado              | Interrompe comando                          |
| TON atingiu preset                  | Ativa saída                                 |
| TOF perdeu entrada                  | Mantém saída pelo preset                    |
| L1 ligado diretamente a L2          | Detecta curto                               |
| Alimentação de motor incompleta     | Não considera motor corretamente alimentado |
| Fase invertida                      | Altera sequência de fases                   |
| Terminal necessário desconectado    | Diagnóstico apropriado                      |
| Dispositivo com referência repetida | Erro de referência                          |

### **Testes de propriedades**

Eu acrescentaria:

**Invariância de ordem**

Permutar componentes e fios, recompilar e comparar os resultados após normalização.

**Reversibilidade**

Executar uma simulação, salvar um checkpoint, avançar, restaurar e reproduzir os mesmos eventos. O trace deverá ser idêntico.

**Determinismo**

Reproduzir a mesma sequência de eventos e obter o mesmo resultado.

**Integridade da topologia**

Desconectar e reconectar fios não pode deixar nós residuais no compilador.

**Segurança do intertravamento**

Nunca permitir que uma condição inválida passe despercebida no circuito reversor ou estrela-triângulo.

### **Um detalhe importante sobre rewind**

Você escreveu:

> State é Clone: cobre replay e rewind.

Eu mudaria essa afirmação.

`Clone` permite criar cópias do estado. Não implementa rewind sozinho.

Para ter rewind real, você precisa de:

```
Input Event Log
      |
      v
Checkpoints
      |
      v
State Restore
      |
      v
Replay
```

Eu armazenaria eventos de entrada por tick e checkpoints periódicos.

Também associaria cada estado a uma versão ou fingerprint do circuito compilado. Um estado antigo não pode ser restaurado cegamente em um `Netlist` diferente.

## **12. Roadmap corrigido**

Eu manteria seus cinco marcos, mas faria alguns ajustes na ordem e nos critérios de conclusão.

## **Roadmap OpenCircuit revisado**

1. **M0 — Contratos e fundação**

   Prioridade máxima
   - Workspace e toolchain.
   - IDs estáveis.
   - Separação Device/SymbolInstance.
   - Terminais estáveis.
   - Modelo de alimentação.
   - Semântica dos ticks.
   - Diagnósticos estruturais.
   - CI em três sistemas operacionais.
   - Teste WASM dos crates portáveis.
   **Critério:** arquitetura e contratos documentados e compilando.
2. **M1 — Motor lógico**
   - Catálogo de componentes.
   - Topologia.
   - Union-Find.
   - Potenciais discretos ou condutores.
   - Bobinas e contatos.
   - Botões.
   - TON e TOF.
   - Circuito direto e reversor.
   - CLI headless.
   **Critério:** circuitos industriais funcionando sem GUI, com testes golden.
3. **M2 — Editor desktop**
   - egui/eframe.
   - Canvas.
   - Biblioteca.
   - Seleção.
   - Edição de fios.
   - Junções.
   - Undo/redo.
   - JSON.
   - Snapshot e visualização da simulação.
   **Critério:** desenhar, salvar, abrir e simular uma partida direta.
4. **M3 — Motores e circuitos avançados**
   - Motor de seis terminais.
   - Enrolamentos.
   - Estrela-triângulo.
   - Solver especializado ou MNA.
   - Diagnósticos elétricos mais completos.
   - Exportação SVG e PDF.
   **Critério:** estrela-triângulo sem sobreposição perigosa de contatores.
5. **M4 — Ecossistema**
   - WASM desktop/web.
   - Integração industrial.
   - Modbus TCP.
   - Extensões WASM.
   - Mais componentes.
   - Documentação e comunidade.
   **Critério:** plataforma extensível e utilizável por terceiros.

Um ajuste importante: os testes de estrela-triângulo podem ser desenvolvidos antecipadamente como cenários de referência, mas não devem ser considerados testes funcionais aprovados antes de existir o modelo de seis terminais.

## **13. Decisões que eu registraria em ADRs**

Antes de programar, criaria um pequeno conjunto de Architectural Decision Records.

São documentos curtos que explicam decisões técnicas importantes, seus motivos e consequências.

Minha proposta:

| ADR     | Decisão                             |
| ------- | ----------------------------------- |
| ADR-001 | Rust e egui/eframe                  |
| ADR-002 | Separação Device e SymbolInstance   |
| ADR-003 | Terminais com IDs estáveis          |
| ADR-004 | Modelo de alimentação e condutores  |
| ADR-005 | Semântica de execução dos ticks     |
| ADR-006 | Solucionador discreto inicial       |
| ADR-007 | Formato JSON e versionamento        |
| ADR-008 | Arquitetura de undo/redo            |
| ADR-009 | Determinismo e replay               |
| ADR-010 | Política de dependências e licenças |

Isso evita que daqui a seis meses alguém precise descobrir por que foi tomada determinada decisão.

# **14. Minha arquitetura final recomendada**

Depois desta revisão, eu manteria os mesmos crates, mas faria a seguinte evolução:

```
OpenCircuit
│
├── apps/
│   ├── desktop/
│   └── cli/
│
├── crates/
│   ├── circuit-domain/
│   │   ├── ids
│   │   ├── device
│   │   ├── symbol
│   │   ├── terminal
│   │   ├── schematic
│   │   └── snapshot
│   │
│   ├── circuit-diagnostics/
│   │
│   ├── circuit-components/
│   │   ├── catalog
│   │   ├── definitions
│   │   └── symbols
│   │
│   ├── circuit-topology/
│   │   ├── compiler
│   │   ├── netlist
│   │   └── signal_table
│   │
│   ├── circuit-electrical/
│   │   ├── conductor
│   │   ├── union_find
│   │   ├── supply
│   │   └── solver
│   │
│   ├── circuit-logic/
│   │   ├── switches
│   │   ├── coils
│   │   ├── timers
│   │   └── devices
│   │
│   ├── circuit-simulation/
│   │   ├── runtime
│   │   ├── clock
│   │   ├── state
│   │   ├── events
│   │   └── replay
│   │
│   ├── circuit-project/
│   ├── circuit-commands/
│   ├── circuit-editor/
│   ├── circuit-renderer/
│   ├── circuit-export/
│   └── circuit-io/
│
├── assets/
├── examples/
└── tests/
```

Não vejo necessidade de criar um crate adicional para cada entidade. Essa organização já oferece uma boa separação sem exagerar na modularidade.

## **15. Decisão final: o que aprovaria e o que bloquearia**

**Aprovaria**

- Rust puro.
- egui/eframe.
- Workspace modular.
- Motor determinístico.
- Netlist compilado.
- Union-Find para conectividade ideal.
- Separação UI/renderização/simulação.
- JSON versionado.
- CLI headless.
- MIT + Apache-2.0.
- Golden tests e CI multiplataforma.

**Não aprovaria ainda**

- `Tag` como identidade de dispositivo.
- `Potential` como representação conjunta de fases, neutro e PE.
- Snapshot sem semântica temporal definida.
- Propagação virtual como solução genérica.
- Motor de seis terminais sem modelo dos enrolamentos.
- Garantia bit a bit para futuros cálculos analógicos.
- Validação das camadas dependendo apenas de cargo-deny.

## **16. Minha conclusão**

O documento tem uma direção técnica muito boa. Eu não faria uma reformulação completa.

O que você precisa agora é uma versão 0.2 com algumas decisões fundamentais mais precisas.

Há uma escolha que considero especialmente acertada: construir o OpenCircuit em torno de um circuito compilado para um modelo de execução, em vez de tentar simular diretamente os objetos gráficos da interface.

Essa arquitetura permitirá futuramente criar um modo de depuração muito interessante: clicar no contato K1 e visualizar qual bobina controla seu estado, quais terminais estão conectados e por que a corrente lógica consegue ou não alcançar determinada carga.

Isso pode ser um diferencial muito forte em relação a ferramentas exclusivamente didáticas de desenho e simulação.

**Minha recomendação de prioridade é esta:**

1. Corrigir Device/Symbol/Tag.
2. Corrigir a representação de fontes e condutores.
3. Formalizar a semântica temporal e o snapshot.
4. Delimitar o que o solver discreto realmente suporta.
5. Congelar as interfaces dos quatro crates de simulação.
6. Só depois começar a implementação da GUI.

Não comece pelo desenho dos símbolos. Comece garantindo que um circuito de partida direta tenha um resultado correto, determinístico e testável.

E uma observação final: simulação de comandos industriais é uma excelente ferramenta didática e de engenharia preliminar, mas o programa não deve alegar validar ou certificar instalações reais sem validação técnica, ensaios e conformidade específicos.

**Próxima entrega que considero mais útil:** transformar esta revisão em uma especificação técnica v0.2, incorporando as correções no modelo de dados, nos contratos Rust, no ciclo de simulação e no grafo de dependências.

Também vale acompanhar a evolução dos primeiros crates e revisar as decisões arquiteturais semanalmente.

🛠️Revisar o progresso do OpenCircuitToca para configurar