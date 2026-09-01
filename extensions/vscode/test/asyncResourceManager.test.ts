import assert from "node:assert/strict";
import test from "node:test";

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

  manager.dispose();
  const resource = new FakeResource();
  resolveResource?.(resource);
  await started;
  await Promise.resolve();

  assert.equal(resource.disposeCount, 1);
  await assert.rejects(manager.get(), /Resource manager is disposed/);
  await assert.rejects(manager.restart(), /Resource manager is disposed/);
});
