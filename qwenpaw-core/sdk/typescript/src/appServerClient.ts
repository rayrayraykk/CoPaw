import {
  type AppProtocolRequests,
  type AppProtocolServerNotifications,
  type ClientInfo,
  type InitializeResponse,
  PROTOCOL_VERSION,
} from "./protocol";
import {
  type CloseHandler,
  type Disposable,
  type NotificationHandler,
  RpcClient,
} from "./rpcClient";

export interface AppServerConnectionOptions {
  readonly clientInfo: ClientInfo;
  readonly requestTimeoutMs?: number;
}

type RequestMethod = keyof AppProtocolRequests;
type NotificationMethod = keyof AppProtocolServerNotifications;
export type AppProtocolNotification = {
  [M in NotificationMethod]: {
    readonly method: M;
    readonly params: AppProtocolServerNotifications[M];
  };
}[NotificationMethod];

export class AppServerClient implements Disposable {
  private constructor(
    private readonly rpc: RpcClient,
    public readonly serverInfo: InitializeResponse["serverInfo"],
  ) {}

  public static async connect(
    input: NodeJS.ReadableStream,
    output: NodeJS.WritableStream,
    options: AppServerConnectionOptions,
  ): Promise<AppServerClient> {
    const rpc = new RpcClient(input, output, options.requestTimeoutMs);
    try {
      const initialized = await rpc.request<InitializeResponse>("initialize", {
        clientInfo: options.clientInfo,
      });
      assertProtocolVersion(initialized);
      rpc.notify("initialized", {});
      return new AppServerClient(rpc, initialized.serverInfo);
    } catch (error) {
      rpc.dispose();
      throw error;
    }
  }

  public request<M extends RequestMethod>(
    method: M,
    params: AppProtocolRequests[M]["params"],
  ): Promise<AppProtocolRequests[M]["result"]> {
    return this.rpc.request(method, params);
  }

  public onNotification<M extends NotificationMethod>(
    method: M,
    handler: (params: AppProtocolServerNotifications[M]) => void,
  ): Disposable {
    return this.rpc.onNotification((candidate, params) => {
      if (candidate === method) {
        handler(params as AppProtocolServerNotifications[M]);
      }
    });
  }

  public onAnyNotification(handler: NotificationHandler): Disposable {
    return this.rpc.onNotification(handler);
  }

  public onEvent(
    handler: (notification: AppProtocolNotification) => void,
  ): Disposable {
    return this.rpc.onNotification((method, params) => {
      handler({ method, params } as AppProtocolNotification);
    });
  }

  public onClose(handler: CloseHandler): Disposable {
    return this.rpc.onClose(handler);
  }

  public dispose(): void {
    this.rpc.dispose();
  }
}

function assertProtocolVersion(response: InitializeResponse): void {
  if (response.protocolVersion !== PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported QwenPaw protocol version: ${response.protocolVersion}; expected ${PROTOCOL_VERSION}`,
    );
  }
}
