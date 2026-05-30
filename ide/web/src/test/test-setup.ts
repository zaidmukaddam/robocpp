import { beforeEach } from "vitest";

const storage = new Map<string, string>();

function installLocalStorage(): void {
  globalThis.localStorage = {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => {
      storage.set(key, value);
    },
    removeItem: (key: string) => {
      storage.delete(key);
    },
    clear: () => storage.clear(),
    key: (index: number) => Array.from(storage.keys())[index] ?? null,
    get length() {
      return storage.size;
    }
  } as Storage;
}

function installCrypto(): void {
  if (!globalThis.crypto?.randomUUID) {
    let counter = 0;
    Object.defineProperty(globalThis, "crypto", {
      value: {
        randomUUID: () => {
          counter += 1;
          return `00000000-0000-4000-8000-${String(counter).padStart(12, "0")}`;
        }
      },
      configurable: true
    });
  }
}

function installUrl(): void {
  if (!globalThis.URL.createObjectURL) {
    globalThis.URL.createObjectURL = () => "blob:mock";
    globalThis.URL.revokeObjectURL = () => undefined;
  }
}

beforeEach(() => {
  storage.clear();
  installLocalStorage();
  installCrypto();
  installUrl();
});
