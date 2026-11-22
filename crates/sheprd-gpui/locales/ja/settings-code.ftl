# Settings > Code: where VS Code tabs' server comes from, its license, the
# check of a server's address, and px0's workbench address.

settings-code-vscode = VS Code
settings-code-start = VS Code を自動的に起動
settings-code-address = アドレスを使用
settings-code-no-pages = このビルドはページを表示しないため、VS Code を起動しません。
settings-code-finding = VS Code を検索中…
settings-code-found = VS Code：{ $path }
settings-code-missing = VS Code が PATH にもアプリケーションフォルダにも見つかりませんでした。アプリから起動するには VS Code をインストールするか、自分で実行しているサーバーのアドレスを使用してください。

settings-code-server = サーバー
settings-code-start-note = VS Code タブで初めて必要になったときにアプリが 127.0.0.1 で `code serve-web` を実行し、アプリの終了時に停止します。
settings-code-idle = まだ起動していません。
settings-code-starting = VS Code を起動中…（初回はダウンロードします）
settings-code-running = { $address } で実行中
settings-code-failed = { $error } 再試行中…
settings-code-license-terms = VS Code のサーバーは Microsoft のソフトウェアです。起動すると「Visual Studio Code Server ライセンス条項」と「Microsoft のプライバシーに関する声明」に同意したことになるため、アプリは同意を得てから起動します。
settings-code-license-link = ライセンス条項
settings-code-privacy-link = プライバシーに関する声明
settings-code-accept = 同意して起動

settings-code-address-note = `code serve-web` でサーバーを起動し、表示されたアドレスを ?tkn= トークンごとペーストしてください。
settings-code-address-invalid = http または https のアドレスを入力してください（例：{ $example }）
settings-code-test = 接続をテスト
settings-code-testing = テスト中…
settings-code-connected = VS Code に接続しました
settings-code-commit = コミット { $commit }

settings-code-px0 = px0
settings-code-px0-placeholder = 自動、または { $example }
settings-code-px0-note = アドレスを空欄にすると、タブを開いたときにローカルのワークベンチを検出するか、インストール済みの px0 を起動します。自分で実行しているワークベンチに接続するには、アドレスを設定してください。
settings-code-px0-invalid = 認証情報、クエリ、フラグメントを含まない、ループバックの HTTP ワークベンチアドレスを使用してください。
settings-code-px0-apply = px0 のアドレスを適用
