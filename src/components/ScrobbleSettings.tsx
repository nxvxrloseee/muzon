import { useEffect, useState } from "react";
import { toast } from "sonner";
import { useScrobbleStore } from "../store/scrobbleStore";
import type { ServiceStatus } from "../types";

const INPUT =
  "w-full rounded border border-divider bg-transparent px-2 py-1.5 text-sm text-text-primary outline-none focus:border-accent-primary";
const BUTTON =
  "rounded-md bg-card-hover px-3 py-1.5 text-xs text-text-primary hover:bg-accent-primary/20 disabled:opacity-50";

/** Pending listens and the last failure, under a connected account. */
function Health({ status }: { status: ServiceStatus }) {
  if (!status.pending && !status.lastError) return null;
  return (
    <div className="mt-2 text-xs">
      {status.pending > 0 && (
        <p className="text-text-secondary">
          Ждут отправки: {status.pending} — уйдут, когда сервис станет доступен
        </p>
      )}
      {status.lastError && <p className="text-accent-secondary">{status.lastError}</p>}
    </div>
  );
}

function Connected({
  name,
  status,
  onDisconnect,
}: {
  name: string;
  status: ServiceStatus;
  onDisconnect: () => void;
}) {
  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <div className="text-sm text-text-primary">
          {name}: <span className="font-medium">{status.user}</span>
        </div>
        <button onClick={onDisconnect} className={BUTTON}>
          Отключить
        </button>
      </div>
      <Health status={status} />
    </>
  );
}

async function attempt(action: () => Promise<void>, done: string) {
  try {
    await action();
    toast.success(done);
  } catch (e) {
    toast.error(String(e));
  }
}

function ListenBrainz() {
  const status = useScrobbleStore((s) => s.status?.listenbrainz);
  const connect = useScrobbleStore((s) => s.connectListenbrainz);
  const disconnect = useScrobbleStore((s) => s.disconnectListenbrainz);
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  if (!status) return null;

  if (status.user) {
    return (
      <Connected
        name="ListenBrainz"
        status={status}
        onDisconnect={() => attempt(disconnect, "ListenBrainz отключён")}
      />
    );
  }

  return (
    <div>
      <div className="mb-1 text-sm text-text-primary">ListenBrainz</div>
      <p className="mb-2 text-xs text-text-secondary">
        Токен — на странице listenbrainz.org/settings, раздел «User token».
      </p>
      <div className="flex gap-2">
        <input
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder="Токен"
          className={INPUT}
        />
        <button
          disabled={busy || !token.trim()}
          onClick={async () => {
            setBusy(true);
            await attempt(() => connect(token), "ListenBrainz подключён");
            setBusy(false);
            setToken("");
          }}
          className={BUTTON}
        >
          {busy ? "Проверяю…" : "Подключить"}
        </button>
      </div>
    </div>
  );
}

function LastFm() {
  const status = useScrobbleStore((s) => s.status?.lastfm);
  const awaiting = useScrobbleStore((s) => s.lastfmAwaitingApproval);
  const begin = useScrobbleStore((s) => s.beginLastfm);
  const finish = useScrobbleStore((s) => s.finishLastfm);
  const disconnect = useScrobbleStore((s) => s.disconnectLastfm);
  const [apiKey, setApiKey] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  if (!status) return null;

  if (status.user) {
    return (
      <Connected
        name="Last.fm"
        status={status}
        onDisconnect={() => attempt(disconnect, "Last.fm отключён")}
      />
    );
  }

  async function run(action: () => Promise<void>, done: string) {
    setBusy(true);
    await attempt(action, done);
    setBusy(false);
  }

  return (
    <div>
      <div className="mb-1 text-sm text-text-primary">Last.fm</div>
      {awaiting ? (
        <>
          <p className="mb-2 text-xs text-text-secondary">
            В браузере открылась страница Last.fm. Разрешите доступ и вернитесь сюда.
          </p>
          <button
            disabled={busy}
            onClick={() => run(finish, "Last.fm подключён")}
            className={BUTTON}
          >
            {busy ? "Проверяю…" : "Я разрешил доступ"}
          </button>
        </>
      ) : (
        <>
          <p className="mb-2 text-xs text-text-secondary">
            Нужны ключи приложения: создайте его на last.fm/api/account/create (адрес
            обратного вызова можно не указывать) и скопируйте API key и Shared secret.
          </p>
          <div className="flex flex-col gap-2">
            <input
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="API key"
              className={INPUT}
            />
            <input
              type="password"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              placeholder="Shared secret"
              className={INPUT}
            />
            <div>
              <button
                disabled={busy || !apiKey.trim() || !secret.trim()}
                onClick={() =>
                  run(() => begin(apiKey, secret), "Открыта страница подтверждения Last.fm")
                }
                className={BUTTON}
              >
                {busy ? "Подключаю…" : "Подключить"}
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

export function ScrobbleSettings() {
  const init = useScrobbleStore((s) => s.init);

  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="mb-8">
      <h2 className="mb-1 text-lg font-semibold text-text-primary">Скробблинг</h2>
      <p className="mb-4 text-sm text-text-secondary">
        Прослушанный трек (половина или 4 минуты) отправляется в подключённые сервисы. Без сети
        прослушивания копятся и уходят позже. Токены хранятся в связке ключей системы.
      </p>
      <div className="flex flex-col gap-4 rounded-lg border border-divider p-4">
        <ListenBrainz />
        <div className="border-t border-divider" />
        <LastFm />
      </div>
    </div>
  );
}
