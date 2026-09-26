import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { commands } from "../bindings";
import type { S3Config, SyncOutcome } from "../types";

/** Приходит событием sync-progress; в командах не встречается, поэтому описан здесь */
export interface SyncProgress {
  phase: "scan" | "upload" | "download" | "library" | "done";
  done: number;
  total: number;
  current: string;
}

/** Все поля конфига заданы: из Rust они приходят необязательными (serde default) */
export type FullS3Config = Required<S3Config>;

type Status = "idle" | "checking" | "syncing";

interface CloudState {
  config: FullS3Config;
  /** Ключи лежат в системном хранилище паролей, сюда они не возвращаются */
  hasCredentials: boolean;
  status: Status;
  progress: SyncProgress | null;
  outcome: SyncOutcome | null;
  error: string | null;
  message: string | null;

  init: () => Promise<void>;
  setField: <K extends keyof FullS3Config>(key: K, value: FullS3Config[K]) => void;
  save: () => Promise<void>;
  saveCredentials: (accessKey: string, secretKey: string) => Promise<void>;
  forgetCredentials: () => Promise<void>;
  check: () => Promise<void>;
  sync: () => Promise<void>;
}

const emptyConfig: FullS3Config = {
  endpoint: "",
  region: "us-east-1",
  bucket: "",
  prefix: "muzon",
  pathStyle: true,
};

export const useCloudStore = create<CloudState>((set, get) => ({
  config: emptyConfig,
  hasCredentials: false,
  status: "idle",
  progress: null,
  outcome: null,
  error: null,
  message: null,

  init: async () => {
    const [config, hasCredentials] = await Promise.all([
      commands.getS3Config(),
      commands.hasS3Credentials(),
    ]);
    set({ config: { ...emptyConfig, ...config }, hasCredentials });

    listen<SyncProgress>("sync-progress", (e) => set({ progress: e.payload }));
    listen<SyncOutcome>("sync-done", (e) => set({ outcome: e.payload }));
  },

  setField: (key, value) => set({ config: { ...get().config, [key]: value } }),

  save: async () => {
    set({ error: null, message: null });
    try {
      await commands.setS3Config(get().config);
      set({ message: "Настройки сохранены" });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  saveCredentials: async (accessKey, secretKey) => {
    set({ error: null, message: null });
    try {
      await commands.setS3Credentials(accessKey, secretKey);
      set({ hasCredentials: true, message: "Ключи сохранены в связке ключей" });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  forgetCredentials: async () => {
    await commands.forgetS3Credentials();
    set({ hasCredentials: false, message: "Ключи удалены" });
  },

  check: async () => {
    set({ status: "checking", error: null, message: null });
    try {
      await commands.setS3Config(get().config); // проверяем то, что видно в полях
      await commands.checkS3Connection();
      set({ message: "Хранилище отвечает, доступ есть" });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ status: "idle" });
    }
  },

  sync: async () => {
    set({ status: "syncing", error: null, message: null, outcome: null, progress: null });
    try {
      await commands.setS3Config(get().config);
      const outcome = await commands.syncNow();
      set({ outcome });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ status: "idle", progress: null });
    }
  },
}));
