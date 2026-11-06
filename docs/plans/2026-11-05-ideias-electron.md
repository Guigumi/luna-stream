# Ideias herdadas do Luna Share (Electron)

> O que o app atual já entrega **para quem usa** e que o Luna Stream deve manter ou melhorar.
> Só ideias e comportamento esperado; como implementar fica para o plano de cada marco.

## Entrar e conectar

- **Login pelo navegador**: o app abre o site, a pessoa entra na conta e volta ao app já conectada. Nada de digitar senha no app.
- **Lembrar a sessão**: abriu o app, já está logado.
- **Achar a sala sozinho**: o app percebe em qual call do Discord a pessoa está e mostra a sala certa.
- **Código de conexão** como alternativa: gerado no site, colado no app.
- **Mensagens de erro claras e em português**: código expirado, sala encerrada, servidor fora do ar, muitas tentativas.

## Transmitir

- **Escolher o que mostrar**: tela inteira ou uma janela, com prévia.
- **Qualidade simples**: escolher resolução (360p a 1080p) e 30/60 fps, e **trocar configs** sem derrubar a transmissão.
- **Fluidez antes de nitidez**: jogo precisa parecer liso, mesmo que perca um pouco de detalhe.
- **Câmera** opcional junto com a tela.

## Áudio

- **Transmitir o som do jogo e dos apps, nunca a voz da call**: quem assiste não ouve eco do Discord.
- **Pegar apps abertos depois**: abriu um jogo no meio da transmissão, o som dele entra sozinho.
- **Funcionar em qualquer Linux**: PipeWire ou PulseAudio.

## Assistir

- **Grade com quem está na sala** e prévia do que cada um está mostrando.
- **Palco**: clicar em alguém para ver em destaque, com opção de tela cheia.
- **Janela flutuante (PiP)** para assistir enquanto joga, lembrando posição e transparência.
- **Volume e mudo por pessoa**.
- **Saber quem está assistindo**: aviso e som quando alguém começa ou para de ver sua tela.

## Extras

- **Status no Discord**: "Transmitindo com a Luna" e o jogo que está rodando.
- **Atualização automática** com aviso e progresso.
- **Tema e preferências** salvos.
- **Uma instância só**: abrir de novo traz a janela existente.

## O que melhorar no Luna Stream

1. **Só gastar ao assistir**: hoje o app recebe o vídeo de todos da sala mesmo sem ninguém assistindo. No novo, a prévia é leve e o vídeo completo só chega quando a pessoa abre o palco.
2. **Sessão guardada com segurança** no cofre de senhas do sistema.
3. **Sala atualizada na hora**, sem ficar perguntando ao site de tempos em tempos.
4. **Privacidade igual à do site**: quando a sala é criptografada, o app também precisa ser.
5. **Áudio mais leve**: reagir quando um app abre ou fecha, em vez de verificar o tempo todo.

## Ideias novas (não existem hoje)

- **Clipe instantâneo** dos últimos segundos com uma tecla, **com** a voz da call e o microfone.
- **Gravação** de sessões inteiras.
- **App que quase não pesa** parado na bandeja.

