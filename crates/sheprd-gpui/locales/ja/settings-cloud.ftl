# Settings > Cloud devices: what the page and every provider's card say
# around their own fields. Provider, machine, region, size, and image names
# come from the providers and stay as they are. The section and the New
# device setup card take their titles from settings.ftl.

settings-cloud-unavailable = Windows ではクラウドデバイスを利用できません。ほかのシステム用のアカウントはここで編集できます。
settings-cloud-remove = 削除
settings-cloud-none-yet = まだありません。

## Shared by the provider cards.

settings-cloud-checking = 確認中…
# A provider card's title, for a provider still being tried out.
settings-cloud-experimental = { $provider }（実験的）
settings-cloud-save = 保存
settings-cloud-cancel = キャンセル
settings-cloud-install-herdr = Herdr がなければインストール
settings-cloud-install-herdr-in-it = Herdr がなければその中にインストール
settings-cloud-new-sandbox = 新規サンドボックス
settings-cloud-create-sandbox = サンドボックスを作成
settings-cloud-finish-setup = セットアップを完了
# A job the main window runs; $name is the new device's name.
settings-cloud-creating = { $name } を作成しています。進行状況はデバイスピッカーに表示されます。
settings-cloud-setting-up = { $name } をセットアップしています。進行状況はデバイスピッカーに表示されます。
settings-cloud-no-window-sandbox = サンドボックスを作成するには、Sheprd のメインウインドウを開いてください。
# An account's API key or token, kept in the credential store.
settings-cloud-api-key = API キー
settings-cloud-forget-key = キーを消去
settings-cloud-key-saved = 保存済みです。置き換えるには新しいキーを入力してください。
settings-cloud-key-missing = API キーが保存されていません。
settings-cloud-key-unreadable = API キーを読み取れませんでした：{ $error }
settings-cloud-key-unusable = API キーを使用できませんでした：{ $error }
settings-cloud-key-store-unavailable = 認証情報ストレージを利用できません。保存済みのデバイスは削除できます。
# The default among size or image choices; $choice is its name.
settings-cloud-default-choice = { $choice }（デフォルト）
# Deleting a device's machine at its provider; $name is the device's name.
settings-cloud-remove-ask = 削除…
settings-cloud-remove-from-herdr = Sheprd からのみ削除
settings-cloud-deleted = { $name } を削除しました。
settings-cloud-delete-failed = { $name } を削除できませんでした：{ $error }

## New device setup.

settings-cloud-setup-run = 接続時にセットアップスクリプトを実行
settings-cloud-setup-note = 新しいクラウドデバイスが初めて接続したときに、セットアップタブで 1 回だけ実行します。実行されるのは、ここでオンにしたエージェント、続いて構成ファイルの [cloud] にあるセットアップスクリプトです。[cloud.coder] のようなプロバイダ別のテーブルがあれば、どちらもそちらに置き換わります。
# $agent is the agent's product name, such as Claude Code.
settings-cloud-setup-install = { $agent } をインストール
settings-cloud-setup-none = 独自のセットアップスクリプトはありません。
# Under the first lines of the user's script, how many more it has.
settings-cloud-setup-more = …ほか { $count } 行
settings-cloud-setup-edit = 構成ファイルでセットアップスクリプトを編集

## Images: each project's sandbox image at a provider.

settings-cloud-images = イメージ
# $provider is the provider's config key, which picks what it calls a machine.
settings-cloud-images-note = { $provider ->
        [daytona] 各プロジェクトのイメージです。プロジェクトの .herdr/sandbox/Dockerfile または開発コンテナに Herdr を加えてビルドします。プロジェクトの新しいサンドボックスはこのイメージから起動し、レシピが変わると新しいイメージをビルドします。ワークスペースのメニューの「Build sandbox images」で事前にビルドできます。
       *[other] 各プロジェクトのイメージです。プロジェクトの .herdr/sandbox/Dockerfile または開発コンテナに Herdr を加えてビルドします。プロジェクトの新しいコンテナはこのイメージから起動し、レシピが変わると新しいイメージをビルドします。ワークスペースのメニューの「Build sandbox images」で事前にビルドできます。
    }
# $project is the project's name, $path its folder.
settings-cloud-image-build-error = { $project }：{ $error } イメージをビルドするには、{ $path } をウインドウで開いてください。
settings-cloud-image-no-window = イメージをビルドするには、Sheprd のメインウインドウを開いてください。
# $image is the image's tag; $built when it was built, an ago- message.
settings-cloud-image-ready = 準備完了 · { $image } · { $built } にビルド
settings-cloud-image-failed = ビルド失敗（{ $built }）
settings-cloud-image-log = ログ
settings-cloud-image-hide-log = ログを非表示
settings-cloud-image-build = 今すぐビルド
settings-cloud-image-reset = リセット
settings-cloud-image-delete = イメージを削除
settings-cloud-image-reset-confirm = ここにあるイメージを削除します。このプロジェクトで次に新しいマシンを作るときに再ビルドされます。確定するには「イメージを削除」をクリックしてください。
