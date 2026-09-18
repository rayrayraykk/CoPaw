export interface DisposableResource {
  dispose(): void | Promise<void>;
  onClose(handler: (error: Error) => void): { dispose(): void };
}

interface ResourceEntry<T> {
  readonly promise: Promise<T>;
}

export class AsyncResourceManager<T extends DisposableResource> {
  private current: ResourceEntry<T> | undefined;
  private disposed = false;
  private retiring: Promise<void> = Promise.resolve();
  private closing: Promise<void> | undefined;

  public constructor(private readonly factory: () => Promise<T>) {}

  public get(): Promise<T> {
    if (this.disposed) {
      return Promise.reject(new Error("Resource manager is disposed"));
    }
    if (this.current) {
      return this.current.promise;
    }
    const entry: ResourceEntry<T> = {
      promise: this.retiring.then(() => {
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

  public restart(): Promise<T> {
    if (this.disposed) {
      return Promise.reject(new Error("Resource manager is disposed"));
    }
    const previous = this.current;
    const previousRetirement = this.retiring;
    this.retiring = previous
      ? previous.promise.then(
        (resource) => resource.dispose(),
        () => previousRetirement,
      )
      : this.retiring;
    const retiring = this.retiring;
    const entry: ResourceEntry<T> = {
      promise: retiring.then(() => {
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
    void this.close().catch(() => undefined);
  }

  public close(): Promise<void> {
    if (this.closing) return this.closing;
    this.disposed = true;
    const current = this.current;
    this.current = undefined;
    this.closing = current
      ? current.promise.then((resource) => resource.dispose(), () => this.retiring)
      : this.retiring;
    return this.closing;
  }

  private invalidate(entry: ResourceEntry<T>): void {
    if (this.current === entry) {
      this.current = undefined;
    }
  }

  private track(entry: ResourceEntry<T>): void {
    void entry.promise.then(
      (resource) => {
        resource.onClose(() => {
          if (this.current !== entry) return;
          this.current = undefined;
          this.retiring = Promise.resolve().then(() => resource.dispose());
          // Retain the rejection for the next get/restart/close without an
          // unhandled rejection while no caller is waiting for this resource.
          void this.retiring.catch(() => undefined);
        });
      },
      () => this.invalidate(entry),
    );
  }
}
