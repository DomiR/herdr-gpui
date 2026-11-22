# Settings > Cloud devices > Container. Engine names (Docker, Podman), their
# versions, and container names stay as they are.

settings-cloud-container-note = このコンピュータの Docker または Podman エンジンでコンテナを作成し、デバイスとして使います。各コンテナには専用のホームボリュームと、CPU、メモリ、プロセス数の上限があり、エージェントはこのコンピュータのファイルにアクセスできない非特権ユーザーとして実行されます。コンテナはこのコンピュータのカーネルを共有するため、信頼できないコードには VM のプロバイダを使ってください。
settings-cloud-container-checking = Docker または Podman を確認しています…
settings-cloud-container-engine-running = { $engine } { $version } が実行中です。

## The engine's fields, each a label and, when not a default value, its placeholder.

settings-cloud-container-engine = エンジン
# "docker" and "podman" are what the field takes; keep them as they are.
settings-cloud-container-engine-placeholder = docker または podman。空欄の場合は最初に見つかったインストール済みのもの
settings-cloud-container-image = イメージ
settings-cloud-container-cpus = CPU 数
settings-cloud-container-memory = メモリ
settings-cloud-container-auto-stop = 自動停止
# "never" and the times are what the field takes; keep them as they are.
settings-cloud-container-auto-stop-placeholder = never、または 30m や 2h：その時間どのエージェントも作業しなければ停止

## New container and Containers.

settings-cloud-container-new = 新規コンテナ
settings-cloud-container-new-note = 自動生成された名前でコンテナを作成し（必要なら先にイメージを取得します）、その中で Herdr が動き始めたらデバイスとして追加します。
settings-cloud-container-install = その中に Herdr をインストール
settings-cloud-container-create = コンテナを作成
settings-cloud-container-no-window = コンテナを作成するには、Sheprd のメインウインドウを開いてください。
settings-cloud-container-devices = コンテナ
settings-cloud-container-devices-note = 「削除」はコンテナの登録だけを解除し、エンジン上には残します。ファイルごと削除するには、サイドバーのメニューの「Remove device…」を使ってください。先にプッシュしていない作業がないか確認されます。
settings-cloud-container-stop = 停止
# A container's row: the engine it runs on, then its state.
settings-cloud-container-running = { $engine } · 実行中
settings-cloud-container-stopped = { $engine } · 停止中
settings-cloud-container-missing = { $engine } · エンジン上に見つかりません
settings-cloud-container-unreachable = { $engine } · エンジンに接続できません
settings-cloud-container-unsaved = デバイスとして未登録
settings-cloud-container-unsaved-note = Sheprd が作成したものの、対応するデバイスがないコンテナです（作成中に終了した場合など）。使うには追加してください。不要なら、追加した後にサイドバーのメニューから削除できます。
settings-cloud-container-adopt = 追加
