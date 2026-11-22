# Settings > Cloud devices > Coder. User, workspace, and deployment names
# come from Coder and stay as they are.

settings-cloud-coder-note = デバイスピッカーから Coder のワークスペースを作成し、デバイスとして使います。下のリダイレクト URI で Coder に OAuth2 アプリを登録してください。デプロイメントで実験的機能 oauth2 を有効にしておく必要があります。
settings-cloud-coder-not-configured = 未設定です。デプロイメントとその OAuth アプリを入力して保存してください。
settings-cloud-coder-waiting-for-browser = ブラウザを待っています…
settings-cloud-coder-finish-in-browser = ブラウザでサインインを完了してください…
settings-cloud-coder-not-signed-in = サインインしていません
settings-cloud-coder-signed-in-as = { $user } としてサインイン中
# $reason is why Coder could not say who is signed in.
settings-cloud-coder-unreadable = サインイン済みですが、アカウントを読み取れませんでした：{ $reason }
settings-cloud-coder-sign-in = サインイン
settings-cloud-coder-sign-out = サインアウト

## The deployment's fields, each a label and its placeholder.

settings-cloud-coder-url = デプロイメント URL
settings-cloud-coder-client-id = OAuth クライアント ID
settings-cloud-coder-client-id-placeholder = OAuth2 アプリのクライアント ID
settings-cloud-coder-redirect = リダイレクト URI
settings-cloud-coder-organization = 組織
settings-cloud-coder-organization-placeholder = 任意。空欄の場合はデフォルトの組織
settings-cloud-coder-prefix = ワークスペース名のプレフィックス
settings-cloud-coder-prefix-placeholder = 任意。空欄の場合は herdr
settings-cloud-coder-cli = coder CLI
settings-cloud-coder-cli-placeholder = 任意。空欄の場合は PATH から検索
settings-cloud-coder-secret = OAuth クライアントシークレット
settings-cloud-coder-secret-environment = HERDR_CODER_OAUTH_CLIENT_SECRET で設定されています（こちらが優先されます）。
settings-cloud-coder-secret-config = 構成ファイルの [coder] oauth_client_secret で設定されています（こちらが優先されます）。
settings-cloud-coder-secret-saved = 保存済みです。置き換えるには新しいシークレットを入力してください。
settings-cloud-coder-secret-missing = まだ保存されていません。Coder へのサインインに必要です。
settings-cloud-coder-forget-secret = シークレットを消去

## Coder devices.

settings-cloud-coder-devices = Coder デバイス
settings-cloud-coder-none-yet = まだありません。「Add Device…」の Coder から追加してください。
settings-cloud-coder-remove-note = 「削除」はデバイスをこのリストから外し、ワークスペースは Coder で実行したままにします。「破棄」は Coder 上のワークスペースを削除します。
settings-cloud-coder-destroy = 破棄…
settings-cloud-coder-destroy-confirm = ワークスペースを破棄
# $workspace is the workspace's name at Coder.
settings-cloud-coder-destroy-warning = 破棄すると、Coder 上のワークスペース { $workspace } とそのディスクが削除され、ここからも削除されます。元に戻すことはできません。
settings-cloud-coder-destroy-contacting = Coder に接続しています…
settings-cloud-coder-destroy-waiting = ワークスペースの現在のビルドが終わるのを待っています…
settings-cloud-coder-destroy-queued = Coder でキューに入りました…
settings-cloud-coder-destroy-deleting = ワークスペースを削除しています…
# $name is the device's name.
settings-cloud-coder-destroyed = { $name } を破棄しました。
settings-cloud-coder-destroy-failed = { $name } を破棄できませんでした：{ $error }
