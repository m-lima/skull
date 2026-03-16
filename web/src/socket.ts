import { encode, decode } from '@msgpack/msgpack';

export class Timeout {
  private readonly millis: number;

  public constructor(millis: number) {
    this.millis = millis;
  }

  public getMillis() {
    return this.millis;
  }
}

export enum SocketState {
  Connecting,
  Open,
  Closed,
  Error,
  Unauthorized,
  Forbidden,
}

class SocketStateListener {
  readonly update: (state: SocketState) => void;

  constructor(handler: (state: SocketState) => void) {
    this.update = handler;
  }
}

export interface Handler<Message> {
  handle(message: Message): boolean;
}

class GlobalHandler<Message> implements Handler<Message> {
  readonly handle: (message: Message) => boolean;

  constructor(handler: (message: Message) => boolean) {
    this.handle = handler;
  }
}

class RequestHandler<Message, Response> implements Handler<Message> {
  readonly id: number;
  readonly handleInner: (message: Message) => Response | undefined;
  readonly accept: (response: Response | PromiseLike<Response>) => void;
  readonly reject: (reason?: unknown) => void;

  constructor(
    id: number,
    handler: (message: Message) => Response | undefined,
    accept: (response: Response | PromiseLike<Response>) => void,
    reject: (reason?: unknown) => void,
    timeout: number,
  ) {
    this.id = id;
    this.handleInner = handler;
    this.accept = accept;
    this.reject = reject;

    setTimeout(() => {
      reject(new Timeout(timeout));
    }, timeout);
  }

  handle(message: Message): boolean {
    try {
      const response = this.handleInner(message);

      if (response) {
        this.accept(response);
        return true;
      } else {
        return false;
      }
    } catch (error) {
      this.reject(error);
      return true;
    }
  }
}

export class Socket {
  private readonly requests: RequestHandler<unknown, unknown>[];
  private readonly handlers: Handler<unknown>[];
  private readonly stateListeners: SocketStateListener[];

  private socket: WebSocket;
  private state: SocketState;
  private attempts: number;
  private lastAttempt: Date;

  public constructor(url: string | URL, checkUrl?: string | URL) {
    this.requests = [];
    this.handlers = [];
    this.stateListeners = [];

    this.state = SocketState.Closed;
    this.attempts = 0;
    this.lastAttempt = new Date(0);
    this.socket = this.connect(url, checkUrl);
  }

  private connect(url: string | URL, checkUrl?: string | URL) {
    this.lastAttempt = new Date();
    this.setState(SocketState.Connecting);

    const socket = new WebSocket(url);
    socket.binaryType = 'arraybuffer';

    socket.addEventListener(
      'error',
      () => {
        this.tryCheckAuthorized(checkUrl);
        this.setState(SocketState.Error);
      },
      false,
    );

    socket.addEventListener(
      'close',
      () => {
        if (this.state !== SocketState.Error) {
          this.setState(SocketState.Closed);
        }

        this.tryReconnect(url, checkUrl);
      },
      false,
    );

    socket.addEventListener(
      'open',
      () => {
        this.setState(SocketState.Open);
      },
      false,
    );

    socket.onmessage = this.onMessage.bind(this);

    this.socket = socket;
    return socket;
  }

  private nextAttempt() {
    // Auth is always fatal
    if (this.state === SocketState.Unauthorized || this.state === SocketState.Forbidden) {
      return;
    }

    const now = new Date();
    if (now.getTime() - this.lastAttempt.getTime() > 60 * 1000) {
      this.attempts = 0;
    }

    switch (this.attempts) {
      case 0:
        return 0;
      case 1:
        return 5 * 1000;
      case 2:
        return 10 * 1000;
      case 3:
        return 15 * 1000;
      default:
        return;
    }
  }

  private tryCheckAuthorized(checkUrl?: string | URL) {
    // Check only in the first failure
    if (this.attempts === 0 && !!checkUrl) {
      fetch(checkUrl, { credentials: 'include', redirect: 'manual' })
        .then(r => {
          if (r.status === 401) {
            this.setState(SocketState.Unauthorized);
          } else if (r.status === 403) {
            this.setState(SocketState.Forbidden);
          }
        })
        .catch(() => {});
    }
  }

  private tryReconnect(url: string | URL, checkUrl?: string | URL) {
    const timeout = this.nextAttempt();

    if (timeout === undefined) {
      return;
    }

    this.attempts += 1;
    setTimeout(() => this.connect(url, checkUrl), timeout);
  }

  private setState(state: SocketState) {
    // If we are now open, gladly accept it
    if (state === SocketState.Open) {
      this.attempts = 0;
      this.state = state;
    } else {
      // Auth can only be overriden by OPEN
      if (this.state === SocketState.Unauthorized || this.state === SocketState.Forbidden) {
        return;
      }

      // If we are authorized and still have attempts, mark it as connecting
      if (
        state !== SocketState.Unauthorized &&
        state !== SocketState.Forbidden &&
        this.nextAttempt() !== undefined
      ) {
        this.state = SocketState.Connecting;
      } else {
        this.state = state;
      }
    }

    for (const listener of this.stateListeners) {
      listener.update(this.state);
    }
  }

  private onMessage(e: MessageEvent) {
    if (!(e.data instanceof ArrayBuffer)) {
      console.error('Received a text message on a binary channel:');
      console.error(e.data);
      return;
    }

    const message = decode(e.data);

    for (let i = 0; i < this.requests.length; ++i) {
      if (this.requests[i].handle(message)) {
        this.requests.splice(i, 1);
        return;
      }
    }

    // TODO: Handle the case where id is zero
    // TODO: Maybe a catch-all handler that creates a pop-up
    for (let i = 0; i < this.handlers.length; ++i) {
      if (this.handlers[i].handle(message)) {
        return;
      }
    }
  }

  public async request<Response>(
    request: unknown,
    handler: (message: unknown) => Response | undefined,
    timeout: number = 30000,
  ): Promise<Response> {
    const payload = encode(request);
    const id = Math.random();
    const promise: Promise<Response> = new Promise((accept, reject) => {
      const requestInstance = new RequestHandler(id, handler, accept, reject, timeout);
      this.requests.push(requestInstance as RequestHandler<unknown, unknown>);
      this.socket.send(payload);
    });

    return promise.finally(() => {
      const index = this.requests.findIndex(r => r.id === id);
      if (index >= 0) {
        this.requests.splice(index, 1);
      }
    });
  }

  public getState() {
    return this.state;
  }

  public registerStateListener(handler: (state: SocketState) => void) {
    const listener = new SocketStateListener(handler);
    this.stateListeners.push(listener);
    return listener;
  }

  public unregisterStateListener(listener: SocketStateListener) {
    const index = this.stateListeners.indexOf(listener);
    if (index >= 0) {
      this.stateListeners.splice(index, 1);
    }
  }

  public registerHandler<Message>(handler: (message: Message) => boolean) {
    const globalHandler = new GlobalHandler(handler);
    this.handlers.push(globalHandler);
    return globalHandler;
  }

  public unregisterHandler<Message>(handler: GlobalHandler<Message>) {
    const index = this.handlers.indexOf(handler);
    if (index >= 0) {
      this.handlers.splice(index, 1);
    }
  }
}
