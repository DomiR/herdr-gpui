# Settings > Cloud devices > Sprites, Fly's persistent machines. Sprite
# names stay as they are.

settings-cloud-sprites-note = Fly の sprite を作成してデバイスとして使います。sprite は永続的な Linux マシンで、アイドル時はスリープし、次のコマンドで復帰します。Sheprd は API の exec ストリームを通じて各 sprite に接続し、その通信はこのアプリが中継します。トークンがこのコンピュータの外に出ることはありません。
settings-cloud-sprites-not-configured = 未設定です。API URL を確認し、トークンを追加してから保存してください。
settings-cloud-sprites-count = 接続済みです。このトークンから、ここで作成した { $count } 個の sprite にアクセスできます。
settings-cloud-sprites-api-url = API URL
settings-cloud-sprites-token = API トークン
settings-cloud-sprites-token-placeholder = Sprites API トークン
settings-cloud-sprites-token-none = API トークンが保存されていません。
settings-cloud-sprites-token-unusable = API トークンを使用できませんでした：{ $error }
settings-cloud-sprites-token-environment = HERDR_SPRITES_TOKEN で設定されています（こちらが優先されます）。
settings-cloud-sprites-token-saved = 保存済みです。置き換えるには新しいトークンを入力してください。
# `sprite org auth` is a command; keep it as it is.
settings-cloud-sprites-token-missing = まだ保存されていません。`sprite org auth` または Sprites ダッシュボードで作成してください。
settings-cloud-sprites-forget-token = トークンを消去

## New sprite and Sprites devices.

settings-cloud-sprites-new = 新規 sprite
settings-cloud-sprites-new-note = 自動生成された名前で sprite を作成し、スリープから毎回復帰するよう Herdr のサーバーをサービスとして実行して、デバイスとして追加します。
settings-cloud-sprites-create = sprite を作成
settings-cloud-sprites-no-window = sprite を作成するには、Sheprd のメインウインドウを開いてください。
settings-cloud-sprites-devices = Sprites デバイス
settings-cloud-sprites-devices-note = 「削除」は sprite の登録だけを解除し、Fly 上には残します。Fly からも削除するには、サイドバーのメニューの「Remove device…」を使ってください。先にプッシュしていない作業がないか確認されます。
