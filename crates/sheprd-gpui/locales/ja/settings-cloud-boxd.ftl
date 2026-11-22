# Settings > Cloud devices > boxd. boxd is a product name and stays
# lowercase. Machine names stay as they are.

settings-cloud-boxd-note = boxd マシンを作成してデバイスとして使います。boxd マシンは永続ディスクを備えた完全な Linux microVM で、約 1 秒で起動し、アイドル時はスリープします。Sheprd は API の exec ストリームを通じて各マシンに接続し、その通信はこのアプリが中継します。API キーがこのコンピュータの外に出ることはありません。
settings-cloud-boxd-not-configured = 未設定です。API URL を確認し、API キーを追加してから保存してください。
settings-cloud-boxd-count = 接続済みです。この API キーから、ここで作成した { $count } 台のマシンにアクセスできます。
settings-cloud-boxd-api-url = API URL
settings-cloud-boxd-key = API キー
settings-cloud-boxd-key-placeholder = boxd API キー
settings-cloud-boxd-key-none = API キーが保存されていません。
settings-cloud-boxd-key-unusable = API キーを使用できませんでした：{ $error }
settings-cloud-boxd-key-environment = HERDR_BOXD_API_KEY で設定されています（こちらが優先されます）。
settings-cloud-boxd-key-saved = 保存済みです。置き換えるには新しいキーを入力してください。
# `boxd auth keys create` is a command; keep it as it is.
settings-cloud-boxd-key-missing = まだ保存されていません。boxd コンソールまたは `boxd auth keys create` で作成してください。
settings-cloud-boxd-forget-key = API キーを消去

## New machine and boxd devices.

settings-cloud-boxd-new = 新規マシン
settings-cloud-boxd-new-note = 自動生成された名前で boxd マシンを作成し、承認されれば Herdr をインストールして、デバイスとして追加します。スタンバイ中または休止状態のマシンは、Sheprd が接続すると復帰します。
settings-cloud-boxd-create = マシンを作成
settings-cloud-boxd-no-window = boxd マシンを作成するには、Sheprd のメインウインドウを開いてください。
settings-cloud-boxd-devices = boxd デバイス
settings-cloud-boxd-devices-note = 「削除」はマシンの登録だけを解除し、boxd 上には残します。boxd からも削除するには、サイドバーのメニューの「Remove device…」を使ってください。先にプッシュしていない作業がないか確認されます。
