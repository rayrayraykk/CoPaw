export interface DisposableResource {
  dispose(): void;
  onClose(handler: (error: Error) => void): { dispose(): void };
}

interface ResourceEntry<T> {
  readonly promise: Promise<T>;
}

export class AsyncResourceManager<T extends DisposableResource> {
  private current: ResourceEntry<T> | undefined;
  private disposed = false;

  public constructor(private readonly factory: () => Promise<T>) {}

  public get(): Promise<T> {
    if (this.disposed) {
      return Promise.reject(new Error("Resource manager is disposed"));
    }
    if (this.current) {
      return this.current.promise;
    }
    const entry: ResourceEntry<T> = {
      promise: Promise.resolve().then(() => this.factory()),
    };
    this.current = entry;
    this.track(entry);
    return entry.promise;
  }

  public restart(): Promise<T> {
    if (this.disposed) {
      return Promise.reject(new Error("Resource manager is disposed"));
    }
    const previous = this.current;
    const entry: ResourceEntry<T> = {
      promise: Promise.resolve().then(async () => {
        if (previous) {
          const resource = await previous.promise.catch(() => undefined);
          resource?.dispose();
        }
        if (this.disposed) {
          throw new Error("Resource manager is disposed");
        }
        return this.factory();
      }),
    };
    this.current = entry;
    this.track(entry);
    return entry.promise;
  }

  public dispose(): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    const current = this.current;
    this.current = undefined;
    if (current) {
      void current.promise.then(
        (resource) => resource.dispose(),
        () => undefined,
      );
    }
  }

  private invalidate(entry: ResourceEntry<T>): void {
    if (this.current === entry) {
      this.current = undefined;
    }
  }

  private track(entry: ResourceEntry<T>): void {
    void entry.promise.then(
      (resource) => {
        resource.onClose(() => this.invalidate(entry));
      },
      () => this.invalidate(entry),
    );
  }
}
