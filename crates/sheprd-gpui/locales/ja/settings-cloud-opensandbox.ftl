# Settings > Cloud devices > OpenSandbox. Server URLs, image names, and
# sandbox names stay as they are.

settings-cloud-opensandbox-note = 自分の OpenSandbox サーバーにサンドボックスを作成してデバイスとして使います。Sheprd は execd ストリームを通じて各サンドボックスに接続し、その通信はこのアプリが中継します。API キーがこのコンピュータの外に出ることはありません。
settings-cloud-opensandbox-not-configured = 未設定です。サーバー URL を確認し、その API キーを追加してから保存してください。
settings-cloud-opensandbox-sandboxes = 接続済みです。サーバーには、ここで作成したサンドボックスが { $count } 個あります。
settings-cloud-opensandbox-unusable = サーバーを使用できませんでした：{ $error }
settings-cloud-opensandbox-key-placeholder = OpenSandbox API キー
settings-cloud-opensandbox-key-environment = HERDR_OPENSANDBOX_API_KEY で設定されています（こちらが優先されます）。
# `api_key` and sandbox.toml are the server's own names; keep them as they are.
settings-cloud-opensandbox-key-missing = まだ保存されていません。サーバーの sandbox.toml にある `api_key` の値です。

## The server's fields; each shows its default as its placeholder.

settings-cloud-opensandbox-url = サーバー URL
settings-cloud-opensandbox-image = イメージ
settings-cloud-opensandbox-cpu = CPU
settings-cloud-opensandbox-memory = メモリ

## New sandbox and OpenSandbox devices.

settings-cloud-opensandbox-new-note = 自動生成された名前で有効期限のないサンドボックスを作成し、そこで Herdr のサーバーを動かし続けて、デバイスとして追加します。
settings-cloud-opensandbox-devices = OpenSandbox デバイス
settings-cloud-opensandbox-devices-note = 「削除」はサンドボックスの登録だけを解除し、サーバー上には残します。サーバーからも削除するには、サイドバーのメニューの「Remove device…」を使ってください。先にプッシュしていない作業がないか確認されます。
