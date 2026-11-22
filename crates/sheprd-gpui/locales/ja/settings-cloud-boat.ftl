# Settings > Cloud devices > Boat. Sandbox, machine, and organization
# names come from Boat and stay as they are.

settings-cloud-boat-note = Boat のサンドボックスを作成してデバイスとして使います。Sheprd は GUI が管理する SSH キーを承認し、Boat の認証済み API から取得した各マシンのホストキーを固定します。
settings-cloud-boat-not-configured = 未設定です。API URL を確認し、API キーを追加してから保存してください。
settings-cloud-boat-sandboxes = 接続済みです。このキーから { $count } 個のサンドボックスにアクセスできます。
settings-cloud-boat-sandboxes-at-least = 接続済みです。このキーから { $count } 個以上のサンドボックスにアクセスできます。
settings-cloud-boat-key-environment = HERDR_BOAT_API_KEY で設定されています（こちらが優先されます）。
settings-cloud-boat-key-missing = まだ保存されていません。Boat ダッシュボードの「API Keys」で作成してください。

## The account's fields, each a label and its placeholder.

settings-cloud-boat-api-url = API URL
settings-cloud-boat-organization = 組織
settings-cloud-boat-organization-placeholder = 任意。空欄の場合は現在の個人または組織のウォレット
settings-cloud-boat-machine-type = マシンタイプ
# The values are what the field takes; keep them as they are.
settings-cloud-boat-machine-type-placeholder = 任意：small、default、large、xlarge のいずれか
settings-cloud-boat-environment = 環境
settings-cloud-boat-environment-placeholder = 任意。空欄の場合はアカウントのデフォルト環境
settings-cloud-boat-auto-stop = 自動停止までの時間
# "never" is what the field takes; keep it as it is.
settings-cloud-boat-auto-stop-placeholder = never、または 1–2592000 秒
settings-cloud-boat-auto-stop-note = 自動停止を無効にするには never を指定します。その場合、Boat でサンドボックスを停止するまで課金が続きます。数値を指定すると、SSH で作業中でも固定の期限になります（トライアルでは 1–7200 秒）。作成時と再開時に適用されます。実行中のデバイスでは、保存してから「セットアップを完了」を使うと、今から期限を延長するか、自動停止を無効にできます。

## New sandbox and Boat devices.

settings-cloud-boat-new-note = 自動生成された名前でサンドボックスを作成します。デバイスはセットアップの前に保存されるため、中断しても別のサンドボックスを作らずに再試行できます。
settings-cloud-boat-recover = 作成を復旧
# $name is the sandbox's generated name.
settings-cloud-boat-pending = 作成待ち：{ $name }。「作成を復旧」は、再起動後でも元のリクエストとオプションを再利用します。復旧は 23 時間で期限切れになり、代わりのサンドボックスを黙って作成することはありません。
settings-cloud-boat-forget-pending = 保留中のリクエストを消去…
settings-cloud-boat-forget-pending-confirm = Boat を確認済み。リクエストを消去
settings-cloud-boat-forget-pending-warning = 先に Boat ダッシュボードを確認してください。元のサンドボックスが存在し、課金が続いている可能性があります。消去されるのはこのローカルのリクエストだけです。もう一度作成すると、2 つ目のサンドボックスが作られることがあります。
settings-cloud-boat-devices = Boat デバイス
settings-cloud-boat-devices-note = 「セットアップを完了」はサンドボックスを再利用し、「自動停止までの時間」を今から適用して、期限を延長するか自動停止を無効にします。上のインストールのスイッチもここに適用されます。セットアップスクリプトは再実行されません。
