import assert from "node:assert/strict";
import test from "node:test";
import { setImmediate } from "node:timers/promises";

import { AsyncResourceManager } from "../src/asyncResourceManager";

class FakeResource {
  public disposeCount = 0;
  private readonly closeHandlers = new Set<(error: Error) => void>();

  public dispose(): void {
    this.disposeCount += 1;
    this.close(new Error("resource disposed"));
  }

  public onClose(handler: (error: Error) => void): { dispose(): void } {
    this.closeHandlers.add(handler);
    return { dispose: () => this.closeHandlers.delete(handler) };
  }

  public close(error = new Error("resource closed")): void {
    for (const handler of this.closeHandlers) {
      handler(error);
    }
    this.closeHandlers.clear();
  }
}

test("shares one pending resource across concurrent callers", async () => {
  let factoryCalls = 0;
  let resolveResource: ((resource: FakeResource) => void) | undefined;
  const pending = new Promise<FakeResource>((resolve) => {
    resolveResource = resolve;
  });
  const manager = new AsyncResourceManager(() => {
    factoryCalls += 1;
    return pending;
  });

  const first = manager.get();
  const second = manager.get();
  const resource = new FakeResource();
  resolveResource?.(resource);

  assert.equal(first, second);
  assert.equal(await first, resource);
  assert.equal(factoryCalls, 1);
  manager.dispose();
});

test("retries after startup failure", async () => {
  const resource = new FakeResource();
  let factoryCalls = 0;
  const manager = new AsyncResourceManager(() => {
    factoryCalls += 1;
    return factoryCalls === 1
      ? Promise.reject(new Error("startup failed"))
      : Promise.resolve(resource);
  });

  await assert.rejects(manager.get(), /startup failed/);

  assert.equal(await manager.get(), resource);
  assert.equal(factoryCalls, 2);
  manager.dispose();
});

test("replaces a resource after its connection closes", async () => {
  const resources = [new FakeResource(), new FakeResource()];
  let factoryCalls = 0;
  const manager = new AsyncResourceManager(() => {
    const resource = resources[factoryCalls];
    factoryCalls += 1;
    if (!resource) {
      return Promise.reject(new Error("unexpected factory call"));
    }
    return Promise.resolve(resource);
  });

  assert.equal(await manager.get(), resources[0]);
  resources[0]?.close();

  assert.equal(await manager.get(), resources[1]);
  assert.equal(factoryCalls, 2);
  manager.dispose();
});

test("a stale close event cannot invalidate a newer resource", async () => {
  const first = new FakeResource();
  const second = new FakeResource();
  const resources = [first, second];
  let factoryCalls = 0;
  const manager = new AsyncResourceManager(() => {
    const resource = resources[factoryCalls];
    factoryCalls += 1;
    return resource
      ? Promise.resolve(resource)
      : Promise.reject(new Error("unexpected factory call"));
  });

  assert.equal(await manager.get(), first);
  assert.equal(await manager.restart(), second);
  assert.equal(first.disposeCount, 1);
  first.close(new Error("late close"));

  assert.equal(await manager.get(), second);
  assert.equal(factoryCalls, 2);
  manager.dispose();
});

test("restart skips a failed startup and creates a new resource", async () => {
  const resource = new FakeResource();
  let factoryCalls = 0;
  const manager = new AsyncResourceManager(() => {
    factoryCalls += 1;
    return factoryCalls === 1
      ? Promise.reject(new Error("startup failed"))
      : Promise.resolve(resource);
  });

  await assert.rejects(manager.get(), /startup failed/);

  assert.equal(await manager.restart(), resource);
  assert.equal(factoryCalls, 2);
  manager.dispose();
});

test("restart and concurrent get share one serialized startup", async () => {
  let resolveFirst: ((resource: FakeResource) => void) | undefined;
  let resolveSecond: ((resource: FakeResource) => void) | undefined;
  const firstPending = new Promise<FakeResource>((resolve) => {
    resolveFirst = resolve;
  });
  const secondPending = new Promise<FakeResource>((resolve) => {
    resolveSecond = resolve;
  });
  let factoryCalls = 0;
  const manager = new AsyncResourceManager(() => {
    factoryCalls += 1;
    return factoryCalls === 1 ? firstPending : secondPending;
  });
  const initial = manager.get();
  await Promise.resolve();

  const restarting = manager.restart();
  const concurrent = manager.get();
  await Promise.resolve();

  assert.equal(restarting, concurrent);
  assert.equal(factoryCalls, 1);
  const first = new FakeResource();
  resolveFirst?.(first);
  assert.equal(await initial, first);
  await Promise.resolve();
  assert.equal(first.disposeCount, 1);
  assert.equal(factoryCalls, 2);
  const second = new FakeResource();
  resolveSecond?.(second);

  assert.equal(await restarting, second);
  assert.equal(factoryCalls, 2);
  manager.dispose();
});

test("dispose releases an active resource exactly once", async () => {
  const resource = new FakeResource();
  const manager = new AsyncResourceManager(() => Promise.resolve(resource));
  assert.equal(await manager.get(), resource);

  manager.dispose();
  manager.dispose();
  await Promise.resolve();

  assert.equal(resource.disposeCount, 1);
});

test("dispose releases a resource whose startup is still pending", async () => {
  let resolveResource: ((resource: FakeResource) => void) | undefined;
  const pending = new Promise<FakeResource>((resolve) => {
    resolveResource = resolve;
  });
  const manager = new AsyncResourceManager(() => pending);
  const started = manager.get();
  await Promise.resolve();

  manager.dispose();
  const resource = new FakeResource();
  resolveResource?.(resource);
  await started;
  await Promise.resolve();

  assert.equal(resource.disposeCount, 1);
  await assert.rejects(manager.get(), /Resource manager is disposed/);
  await assert.rejects(manager.restart(), /Resource manager is disposed/);
});

test("restart waits for asynchronous disposal before replacement", async () => {
  let release!: () => void;
  const finished = new Promise<void>((resolve) => { release = resolve; });
  class DrainingResource extends FakeResource {
    public override async dispose(): Promise<void> {
      await finished;
      super.dispose();
    }
  }
  const first = new DrainingResource();
  const second = new FakeResource();
  let calls = 0;
  const manager = new AsyncResourceManager<FakeResource>(async () =>
    ++calls === 1 ? first : second);
  await manager.get();
  const replacement = manager.restart();
  try {
    await setImmediate();
    assert.deepEqual({ calls, disposed: first.disposeCount }, { calls: 1, disposed: 0 });
  } finally {
    release();
    await replacement;
    manager.dispose();
  }
  assert.equal(first.disposeCount, 1);
});

test("connection loss drains the resource before a new get", async () => {
  let release!: () => void;
  const finished = new Promise<void>((resolve) => { release = resolve; });
  class DrainingResource extends FakeResource {
    public override async dispose(): Promise<void> {
      await finished;
      super.dispose();
    }
  }
  const first = new DrainingResource();
  const second = new FakeResource();
  let calls = 0;
  const manager = new AsyncResourceManager<FakeResource>(async () =>
    ++calls === 1 ? first : second);
  await manager.get();
  first.close();
  const replacement = manager.get();
  try {
    await setImmediate();
    assert.deepEqual({ calls, disposed: first.disposeCount }, { calls: 1, disposed: 0 });
  } finally {
    release();
    await replacement;
    manager.dispose();
  }
  assert.equal(first.disposeCount, 1);
});

test("close shares asynchronous release and rejects new admission", async () => {
  let release!: () => void;
  const finished = new Promise<void>((resolve) => { release = resolve; });
  class Resource extends FakeResource {
    public override async dispose(): Promise<void> {
      await finished;
      super.dispose();
    }
  }
  const resource = new Resource();
  const manager = new AsyncResourceManager(async () => resource);
  await manager.get();
  const closing = manager.close();
  assert.equal(manager.close(), closing);
  await assert.rejects(manager.get(), /disposed/);
  await assert.rejects(manager.restart(), /disposed/);
  assert.equal(resource.disposeCount, 0);
  release();
  await closing;
  assert.equal(resource.disposeCount, 1);
});

test("failed release is retained and never starts a replacement", async () => {
  const error = new Error("Core exited with code 1");
  class Resource extends FakeResource {
    public override async dispose(): Promise<void> { throw error; }
  }
  let calls = 0;
  const manager = new AsyncResourceManager(async () => { calls++; return new Resource(); });
  await manager.get();
  await assert.rejects(manager.restart(), (actual) => actual === error);
  await assert.rejects(manager.get(), (actual) => actual === error);
  await assert.rejects(manager.close(), (actual) => actual === error);
  assert.equal(calls, 1);
});

test("close waits for a pending restart without spawning another resource", async () => {
  let release!: () => void;
  const finished = new Promise<void>((resolve) => { release = resolve; });
  class Resource extends FakeResource {
    public override async dispose(): Promise<void> { await finished; super.dispose(); }
  }
  let calls = 0;
  const resource = new Resource();
  const manager = new AsyncResourceManager(async () => { calls++; return resource; });
  await manager.get();
  const restart = assert.rejects(manager.restart(), /disposed/);
  const closing = manager.close();
  release();
  await Promise.all([restart, closing]);
  assert.deepEqual({ calls, disposed: resource.disposeCount }, { calls: 1, disposed: 1 });
});

test("closing before factory admission does not start a resource", async () => {
  let calls = 0;
  const manager = new AsyncResourceManager(async () => { calls++; return new FakeResource(); });
  const start = assert.rejects(manager.get(), /disposed/);
  await manager.close();
  await start;
  assert.equal(calls, 0);
});

test("reentrant close handlers receive the same closing result", async () => {
  const resource = new FakeResource();
  const manager = new AsyncResourceManager(async () => resource);
  await manager.get();
  let reentrant: Promise<void> | undefined;
  resource.onClose(() => { reentrant = manager.close(); });
  const closing = manager.close();
  await closing;
  assert.equal(reentrant, closing);
  assert.equal(resource.disposeCount, 1);
});

test("unexpected connection loss retains a failed process cleanup", async () => {
  const error = new Error("shutdown failed");
  class Resource extends FakeResource {
    public override async dispose(): Promise<void> { throw error; }
  }
  const resource = new Resource();
  let calls = 0;
  const manager = new AsyncResourceManager(async () => { calls++; return resource; });
  await manager.get();
  resource.close();
  await setImmediate();
  await assert.rejects(manager.get(), (actual) => actual === error);
  await assert.rejects(manager.close(), (actual) => actual === error);
  assert.equal(calls, 1);
});
