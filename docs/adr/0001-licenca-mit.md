# ADR-0001: Licença MIT

- **Status:** aceita
- **Data:** 2026-11-05

## Contexto

O motor será público, mas precisa ser usado por produtos fechados da Luna (bot e site). Também foi considerado usar código do gpu-screen-recorder (GPL-3.0).

## Decisão

- O repositório usa **MIT**.
- **Nenhum código GPL** entra neste repositório. O gpu-screen-recorder é só inspiração conceitual.
- Dependências devem ser compatíveis com uso fechado: MIT/Apache/BSD, ou LGPL com linkagem dinâmica (por exemplo, GStreamer).

## Consequências

- O motor pode ser embutido no share-client e em outros produtos fechados.
- Qualquer componente GPL futuro precisa ficar num repositório separado e ser usado só como processo externo.
