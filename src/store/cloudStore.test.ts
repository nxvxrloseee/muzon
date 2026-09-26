import { beforeEach, describe, expect, it, vi } from "vitest";

const calls = {
  getS3Config: vi.fn(),
  hasS3Credentials: vi.fn(),
  setS3Config: vi.fn(),
  setS3Credentials: vi.fn(),
  forgetS3Credentials: vi.fn(),
  checkS3Connection: vi.fn(),
  syncNow: vi.fn(),
};

vi.mock("../bindings", () => ({ commands: calls }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const { useCloudStore } = await import("./cloudStore");

const initial = useCloudStore.getState();

beforeEach(() => {
  useCloudStore.setState(initial, true);
  Object.values(calls).forEach((c) => c.mockReset());
});

describe("настройки хранилища", () => {
  it("дополняет неполный конфиг значениями по умолчанию", async () => {
    // Rust отдаёт поля необязательными: в старом s3.json их может не быть
    calls.getS3Config.mockResolvedValue({ bucket: "music" });
    calls.hasS3Credentials.mockResolvedValue(true);

    await useCloudStore.getState().init();

    const { config, hasCredentials } = useCloudStore.getState();
    expect(config.bucket).toBe("music");
    expect(config.prefix).toBe("muzon");
    expect(config.region).toBe("us-east-1");
    expect(config.pathStyle).toBe(true);
    expect(hasCredentials).toBe(true);
  });

  it("сохраняет то, что видно в полях, перед проверкой подключения", async () => {
    calls.setS3Config.mockResolvedValue(null);
    calls.checkS3Connection.mockResolvedValue(null);

    useCloudStore.getState().setField("bucket", "music");
    await useCloudStore.getState().check();

    expect(calls.setS3Config).toHaveBeenCalledWith(
      expect.objectContaining({ bucket: "music" }),
    );
    expect(useCloudStore.getState().message).toContain("доступ есть");
    expect(useCloudStore.getState().status).toBe("idle");
  });

  it("показывает ошибку хранилища и не остаётся в состоянии проверки", async () => {
    calls.setS3Config.mockResolvedValue(null);
    calls.checkS3Connection.mockRejectedValue("403 AccessDenied");

    await useCloudStore.getState().check();

    expect(useCloudStore.getState().error).toContain("AccessDenied");
    expect(useCloudStore.getState().status).toBe("idle");
  });

  it("после синхронизации показывает итог и сбрасывает прогресс", async () => {
    calls.setS3Config.mockResolvedValue(null);
    calls.syncNow.mockResolvedValue({
      uploaded: 3,
      downloaded: 1,
      upToDate: 40,
      bytesUp: 1000,
      bytesDown: 10,
      failures: [],
    });

    await useCloudStore.getState().sync();

    const { outcome, progress, status } = useCloudStore.getState();
    expect(outcome?.uploaded).toBe(3);
    expect(progress).toBeNull();
    expect(status).toBe("idle");
  });

  it("помнит, что ключи сохранены, и что их забыли", async () => {
    calls.setS3Credentials.mockResolvedValue(null);
    calls.forgetS3Credentials.mockResolvedValue(undefined);

    await useCloudStore.getState().saveCredentials("AKIA", "secret");
    expect(useCloudStore.getState().hasCredentials).toBe(true);
    expect(calls.setS3Credentials).toHaveBeenCalledWith("AKIA", "secret");

    await useCloudStore.getState().forgetCredentials();
    expect(useCloudStore.getState().hasCredentials).toBe(false);
  });
});
