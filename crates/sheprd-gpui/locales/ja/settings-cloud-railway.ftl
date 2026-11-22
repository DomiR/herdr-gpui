# Settings > Cloud devices > Railway: no account, only the SSH key new VMs
# are created with. Paths, key names, and machine names stay as they are.

settings-cloud-railway-note = railway.new の一時的な VM です。下の SSH キーで作成されます。
settings-cloud-railway-unavailable = Windows ではクラウドデバイスを利用できません。
settings-cloud-railway-key = SSH キー
settings-cloud-railway-source-generate = 新しいキーを生成
settings-cloud-railway-source-existing = 既存のキーを使用
settings-cloud-railway-key-name = キー名
# What a key's name may be; .pub is the public key's file extension.
settings-cloud-railway-name-rule = 英数字、ドット、アンダースコア、ハイフンを 1〜64 文字で指定してください。ドットやハイフンで始めることと、.pub で終えることはできません。
# Where a generated key goes: $folder is a folder, $name the key's file name.
settings-cloud-railway-key-location = { $folder }/{ $name } と { $name }.pub を作成します。
settings-cloud-railway-key-location-no-ssh = ~/.ssh フォルダがないため、{ $folder }/{ $name } と { $name }.pub を作成します。
settings-cloud-railway-key-location-checking = キーの保存場所を確認しています…
settings-cloud-railway-generate = キーを生成
settings-cloud-railway-key-created = { $path } を作成しました。
settings-cloud-railway-key-path = 秘密鍵のパス
settings-cloud-railway-key-path-note = このファイルが変更・削除されることはありません。パスフレーズで保護されたキーには、同じ場所にある .pub ファイルと、キーを読み込んだ SSH エージェントが必要です。
settings-cloud-railway-browse = 参照…
# The title of the system file dialog.
settings-cloud-railway-choose-key = SSH 秘密鍵を選択
settings-cloud-railway-install = VM に Herdr がなければインストール
settings-cloud-railway-create = 接続してセットアップ
settings-cloud-railway-busy = 実行中の Railway の操作が終わるまでお待ちください。
settings-cloud-railway-key-needed = キーを生成するか、既存のキーのパスを入力してください。
settings-cloud-railway-connecting = { $name } に接続しています。進行状況はデバイスピッカーに表示されます。
settings-cloud-railway-no-window = VM をセットアップするには、Sheprd のメインウインドウを開いてください。

## Railway devices: each one's machine, then the key it uses.

settings-cloud-railway-devices = Railway デバイス
settings-cloud-railway-device = { $machine } · { $key }
settings-cloud-railway-device-key-missing = { $machine } · { $key } · キーファイルがありません
