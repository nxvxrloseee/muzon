import { useEffect, useState } from "react";
import { useCloudStore } from "../store/cloudStore";
import type { FullS3Config } from "../store/cloudStore";

/// Адрес и регион у каждого провайдера свои; path-style нужен всем, кроме AWS.
const PRESETS: { name: string; endpoint: string; region: string; pathStyle: boolean; hint?: string }[] = [
  {
    name: "Yandex Cloud",
    endpoint: "https://storage.yandexcloud.net",
    region: "ru-central1",
    pathStyle: true,
  },
  { name: "AWS", endpoint: "", region: "us-east-1", pathStyle: false },
  {
    name: "MinIO",
    endpoint: "http://localhost:9000",
    region: "us-east-1",
    pathStyle: true,
    hint: "Замените адрес на свой сервер",
  },
  {
    name: "Backblaze B2",
    endpoint: "https://s3.us-west-004.backblazeb2.com",
    region: "us-west-004",
    pathStyle: true,
    hint: "Регион и адрес смотрите в консоли B2",
  },
  {
    name: "Cloudflare R2",
    endpoint: "https://ИДЕНТИФИКАТОР.r2.cloudflarestorage.com",
    region: "auto",
    pathStyle: true,
    hint: "Подставьте идентификатор аккаунта в адрес",
  },
];

const PHASES: Record<string, string> = {
  scan: "Сверяю с хранилищем",
  upload: "Отправляю",
  download: "Скачиваю",
  library: "Библиотека",
  done: "Готово",
};

function Field({
  label,
  hint,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  hint?: string;
  value: string;
  placeholder?: string;
  onChange: (v: string) => void;
}) {
  return (
    <label className="block">
      <span className="text-sm text-text-secondary">{label}</span>
      <input
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="mt-1 w-full rounded border border-divider bg-transparent px-2 py-1.5 text-sm text-text-primary"
      />
      {hint && <span className="mt-1 block text-xs text-text-secondary/80">{hint}</span>}
    </label>
  );
}

function megabytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} КБ`;
  const mb = bytes / (1024 * 1024);
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} ГБ` : `${mb.toFixed(1)} МБ`;
}

export function CloudSettings() {
  const config = useCloudStore((s) => s.config);
  const hasCredentials = useCloudStore((s) => s.hasCredentials);
  const status = useCloudStore((s) => s.status);
  const progress = useCloudStore((s) => s.progress);
  const outcome = useCloudStore((s) => s.outcome);
  const error = useCloudStore((s) => s.error);
  const message = useCloudStore((s) => s.message);
  const init = useCloudStore((s) => s.init);
  const setField = useCloudStore((s) => s.setField);
  const save = useCloudStore((s) => s.save);
  const saveCredentials = useCloudStore((s) => s.saveCredentials);
  const forgetCredentials = useCloudStore((s) => s.forgetCredentials);
  const check = useCloudStore((s) => s.check);
  const sync = useCloudStore((s) => s.sync);

  const [presetHint, setPresetHint] = useState<string | null>(null);
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");

  useEffect(() => {
    void init();
  }, [init]);

  const busy = status !== "idle";
  const ready = config.bucket.trim() !== "" && hasCredentials;

  const field = (key: keyof FullS3Config) => (v: string) => setField(key, v as never);

  return (
    <section className="mb-8">
      <h2 className="mb-1 text-lg font-semibold text-text-primary">Облако (S3)</h2>
      <p className="mb-4 text-sm text-text-secondary">
        Фонотека зеркалится в хранилище с API S3 — AWS, MinIO, Backblaze B2, R2, Object Storage.
        Синхронизация двусторонняя: недостающее докачивается, но ничего никогда не удаляется.
      </p>

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <span className="text-xs text-text-secondary">Заполнить для:</span>
        {PRESETS.map((preset) => (
          <button
            key={preset.name}
            onClick={() => {
              setField("endpoint", preset.endpoint);
              setField("region", preset.region);
              setField("pathStyle", preset.pathStyle);
              setPresetHint(preset.hint ?? null);
            }}
            className="rounded-md bg-card-background px-2.5 py-1 text-xs text-text-primary hover:bg-card-hover"
          >
            {preset.name}
          </button>
        ))}
      </div>
      {presetHint && <p className="mb-3 text-xs text-text-secondary">{presetHint}</p>}

      <div className="grid gap-3 rounded-md bg-card-background p-4 md:grid-cols-2">
        <Field
          label="Бакет"
          value={config.bucket}
          placeholder="my-music"
          onChange={field("bucket")}
        />
        <Field
          label="Папка в бакете"
          value={config.prefix}
          placeholder="muzon"
          onChange={field("prefix")}
        />
        <Field
          label="Адрес"
          hint="Пусто — AWS в указанном регионе"
          value={config.endpoint}
          placeholder="https://s3.eu-central-1.amazonaws.com"
          onChange={field("endpoint")}
        />
        <Field label="Регион" value={config.region} placeholder="us-east-1" onChange={field("region")} />

        <label className="flex items-center gap-2 md:col-span-2">
          <input
            type="checkbox"
            checked={config.pathStyle}
            onChange={(e) => setField("pathStyle", e.target.checked)}
            className="h-4 w-4"
          />
          <span className="text-sm text-text-secondary">
            Адрес вида <code>хост/бакет/ключ</code> — нужен MinIO и большинству своих серверов;
            для AWS снимите галочку
          </span>
        </label>
      </div>

      <div className="mt-3 rounded-md bg-card-background p-4">
        <div className="mb-2 text-sm text-text-primary">
          Ключи доступа
          <span className="ml-2 text-xs text-text-secondary">
            {hasCredentials ? "сохранены в связке ключей системы" : "не заданы"}
          </span>
        </div>
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="Access key" value={accessKey} onChange={setAccessKey} />
          <label className="block">
            <span className="text-sm text-text-secondary">Secret key</span>
            <input
              type="password"
              value={secretKey}
              onChange={(e) => setSecretKey(e.target.value)}
              className="mt-1 w-full rounded border border-divider bg-transparent px-2 py-1.5 text-sm text-text-primary"
            />
          </label>
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            onClick={async () => {
              await saveCredentials(accessKey, secretKey);
              setAccessKey("");
              setSecretKey("");
            }}
            disabled={!accessKey || !secretKey}
            className="rounded-md bg-card-hover px-3 py-1.5 text-xs text-text-primary disabled:opacity-40"
          >
            Сохранить ключи
          </button>
          {hasCredentials && (
            <button
              onClick={() => forgetCredentials()}
              className="rounded-md px-3 py-1.5 text-xs text-text-secondary hover:bg-card-hover"
            >
              Забыть ключи
            </button>
          )}
        </div>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <button
          onClick={() => save()}
          disabled={busy}
          className="rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover disabled:opacity-40"
        >
          Сохранить настройки
        </button>
        <button
          onClick={() => check()}
          disabled={busy || !ready}
          className="rounded-md bg-card-background px-3 py-1.5 text-xs text-text-primary hover:bg-card-hover disabled:opacity-40"
        >
          {status === "checking" ? "Проверяю…" : "Проверить подключение"}
        </button>
        <button
          onClick={() => sync()}
          disabled={busy || !ready}
          className="rounded-md bg-accent-primary/20 px-3 py-1.5 text-xs text-text-primary hover:bg-accent-primary/30 disabled:opacity-40"
        >
          {status === "syncing" ? "Синхронизирую…" : "Синхронизировать"}
        </button>
      </div>

      {progress && (
        <div className="mt-3 rounded-md bg-card-background p-3">
          <div className="flex justify-between text-xs text-text-secondary">
            <span>
              {PHASES[progress.phase] ?? progress.phase}
              {progress.current && `: ${progress.current}`}
            </span>
            {progress.total > 0 && (
              <span>
                {progress.done} / {progress.total}
              </span>
            )}
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-progress-track">
            <div
              className="h-full rounded-full bg-progress-fill transition-all"
              style={{
                width: progress.total > 0 ? `${(progress.done / progress.total) * 100}%` : "100%",
              }}
            />
          </div>
        </div>
      )}

      {outcome && (
        <div className="mt-3 rounded-md bg-card-background p-3 text-xs text-text-secondary">
          Отправлено: {outcome.uploaded} ({megabytes(outcome.bytesUp)}) · Скачано:{" "}
          {outcome.downloaded} ({megabytes(outcome.bytesDown)}) · Совпадало: {outcome.upToDate}
          {outcome.failures.length > 0 && (
            <ul className="mt-2 list-disc pl-4 text-accent-secondary">
              {outcome.failures.slice(0, 5).map((f) => (
                <li key={f}>{f}</li>
              ))}
              {outcome.failures.length > 5 && <li>…и ещё {outcome.failures.length - 5}</li>}
            </ul>
          )}
        </div>
      )}

      {message && <p className="mt-2 text-xs text-text-secondary">{message}</p>}
      {error && <p className="mt-2 text-xs text-accent-secondary">{error}</p>}
    </section>
  );
}
