# The Settings window: its frame, section list, search, and footer, and the
# title and description of every row search can find (catalog.rs), which
# the pages show the rows by too. Each page's other text has its own file.

settings-title = 設定
settings-search-placeholder = 設定を検索
settings-sections-label = 設定のセクション
settings-sections = セクション
settings-results = 検索結果
settings-search-empty = 一致する設定はありません。
# A search result: the page it is on, then the row's title.
settings-search-result = { $section } › { $title }
settings-loading = 環境設定を読み込んでいます…
settings-finishing = 閉じる前に環境設定を確定しています…
settings-saved = すべての変更を保存しました。
settings-saved-cloud = アカウントの項目は「保存」で保存されます。サインインと削除はすぐに反映されます。
# $drafts lists the unsaved parts, each one of the settings-draft- messages.
settings-unsaved = 未保存：{ $drafts }（このウインドウを閉じると保存されます）
settings-draft-theme = テーマ
settings-draft-layout = レイアウト
settings-draft-sidebar = サイドバー
settings-save-now = 今すぐ保存
settings-reload = 再読み込み
settings-error-system-appearance = システムの外観を適用できませんでした：{ $error }
# Which app a setting belongs to, as a tag beside it.
settings-source-herdr = Herdr
settings-source-herdr-ui = Sheprd
# The footer after a save: what went wrong saving or reading the files back.
settings-saved-unreloaded = 保存しましたが、現在の環境設定を再読み込みできませんでした。
settings-save-failed-reloaded = 保存に失敗したため、現在の環境設定を再読み込みしました。
settings-save-failed = 保存に失敗しました。再読み込みしてから編集してください。
settings-saved-daemon-unqueued = 保存しましたが、デーモンの再読み込みをキューに追加できませんでした：{ $error }
settings-error-save = 設定を保存できませんでした：{ $error }
settings-error-reload = 設定を再読み込みできなかったため、現在の環境設定を維持します：{ $error }
settings-error-load-herdr = Herdr の設定を読み込めませんでした：{ $error }

## Shared by the pages' controls.

# Why rows that edit Herdr's own file are disabled.
settings-shared-read-only = このプラットフォームでは Herdr の設定は読み取り専用です。
settings-shared-unavailable = Herdr の設定を利用できません。再読み込みして再試行してください。
# The first entry of a dropdown that can follow Herdr's file, with the
# value Herdr has; the other entries override it.
settings-use-herdr = Herdr の設定を使用 · { $value }
settings-on = オン
settings-off = オフ
# Where a notification appears in the window.
settings-position-top-left = 左上
settings-position-top-center = 上中央
settings-position-top-right = 右上
settings-position-bottom-left = 左下
settings-position-bottom-center = 下中央
settings-position-bottom-right = 右下
# Under a number field with steppers: the values it takes.
settings-number-range = { $min }〜{ $max }
# Units after a number field's value.
settings-unit-percent = %
settings-unit-pixels = px
settings-unit-seconds = 秒
# A host with no colour of its own, in place of a colour's hex code.
settings-swatch-none = なし

## Groups of pages in the section list.

settings-group-appearance = 外観
settings-group-behaviour = 動作
settings-group-connect = 接続
settings-group-advanced = 詳細

## Pages, and the line under each page's title.

settings-section-general = 一般
settings-section-theme = テーマ
settings-section-sidebar = サイドバー
settings-section-indicators = インジケーター
settings-section-fonts = フォント
settings-section-status-bar = ステータスバー
settings-section-tabs-and-panes = タブとペイン
settings-section-chat = チャット
settings-section-code = コード
settings-section-notifications = 通知
settings-section-integrations = 連携
settings-section-plugins = プラグイン
settings-section-cloud-devices = クラウドデバイス
settings-section-machines = 仮想 Mac
settings-section-remote-control = リモートコントロール
settings-section-configuration = 構成
settings-lead-theme = テーマにポインタを合わせるとアプリで試せます。クリックすると適用されます。
settings-lead-sidebar = 変更は開いているすべてのウインドウにすぐ反映されます。
settings-lead-indicators = サイドバー、タブ、折りたたんだレールでのエージェントのステータスの表示方法です。作業中のアイコンは常に動きますが、動きを減らす設定では静止します。
settings-lead-fonts = 表示箇所ごとにフォントファミリーとサイズを設定できます。
settings-lead-chat = エージェントのチャットの見た目、開始する場所、コードブロックの初期状態です。各コードブロックのボタンでは、そのブロックだけを変更できます。
settings-lead-status-bar = 下部のバーに表示する項目と、表示の簡潔さです。
settings-lead-code = Visual Studio Code サーバーを使って、ターミナルの横でコードと差分をレビューします。
settings-lead-integrations = Herdr が起動・追跡できるエージェントです。
settings-lead-plugins = プラグインが報告する内容をサイドバーに表示します。
settings-lead-cloud-devices = クラウドアカウントからマシンを作成し、デバイスとして使用します。
settings-lead-machines = タブ内で動作する Mac です。タブを閉じても一時停止するまで動作し続け、アプリを終了すると状態が保存されます。
settings-lead-remote-control = スマートフォンからこのコンピュータのチャット、ターミナル、通知を、ネットワーク、Tailscale、またはリレー経由で使用できます。Fleet はこの機能の上で動作します。
settings-lead-configuration = これらの設定の元になるファイルです。保存した編集は自動的に再読み込みされます。

## Rows, in catalog order: each title, then its description.

settings-theme-mode = 外観
settings-theme-mode-description = ライトとダークのテーマを選択するか、システムに合わせます。
settings-theme-side = 編集対象
settings-theme-side-description = 下のテーマ一覧で、ライトとダークのどちらを変更するかを選びます。
settings-theme-follow = Herdr のテーマに従う
settings-theme-follow-description = GUI での選択の代わりに、Herdr 自身の構成で指定されたテーマを使用します。
settings-theme-contrast = ハイコントラスト
settings-theme-contrast-description = すべてのテーマでテキストとマーカーのコントラストを上げます。
settings-theme-source = ソース
settings-theme-source-description = Ghostty のテーマはこのアプリに適用され、Herdr のテーマは共有されます。
settings-theme-backdrop = 背景
settings-theme-backdrop-description = メインウインドウの背後に表示するもの（単色、半透明、画像）です。
settings-theme-padding-left = 左
settings-theme-padding-left-description = サイドバー側のターミナルの余白です。
settings-theme-padding-right = 右
settings-theme-padding-right-description = 右側のターミナルの余白です。
settings-theme-padding-top = 上
settings-theme-padding-top-description = ターミナルの上の余白です。
settings-theme-padding-bottom = 下
settings-theme-padding-bottom-description = ターミナルの下の余白です。
settings-sidebar-layout = レイアウトプリセット
settings-sidebar-layout-description = 行の形と密度です。下の間隔の設定は、プリセットの値が初期値になります。
settings-sidebar-select = 選択時の強調
settings-sidebar-select-description = ワークスペースを選択したときに、その行以外で強調表示する部分です。
settings-sidebar-device-footer = デバイスフッター
settings-sidebar-device-footer-description = デバイスピッカーとそのボタンです。「自動」では、デバイスが 2 台以上あるときに表示されます。
settings-sidebar-change-counts = 変更行数
settings-sidebar-change-counts-description = ワークツリーの下に表示する +追加/-削除の行数です。プルリクエストの行数、または HEAD に対する未コミットの変更の行数を示します。
settings-sidebar-show-agents = エージェントを表示
settings-sidebar-show-agents-description = 実行中のエージェントをサイドバーに一覧表示します。
settings-sidebar-agents-placement = エージェントの配置
settings-sidebar-agents-placement-description = ワークスペースの下の専用セクション、または各エージェントが実行されているワークスペースの下に表示します。
settings-sidebar-show-agents-header = エージェントのヘッダーを表示
settings-sidebar-show-agents-header-description = 並べ替えの切り替えがある「Agents」のタイトル行です。
settings-sidebar-show-search = 検索を表示
settings-sidebar-show-search-description = ワークスペースの上にある検索フィールドです。
settings-sidebar-repository-icons = リポジトリのアイコンを表示
settings-sidebar-repository-icons-description = 各ワークスペース行の先頭に表示する、リポジトリ所有者のアバターです。
settings-sidebar-device-icons = デバイスのアイコンを表示
settings-sidebar-device-icons-description = 各デバイス名の前に付くマーク（このマシン、クラウドプロバイダ、SSH）です。
settings-sidebar-gpu-load = GPU とビデオメモリを表示
settings-sidebar-gpu-load-description = 各ホストの GPU 使用率と VRAM を CPU とメモリの下の行に表示します。コンパクトなレイアウトでは名前の後に GPU ゲージを表示します。デフォルトはオフです。
settings-sidebar-indent = インデント
settings-sidebar-indent-description = 階層が 1 段深くなるごとに字下げするピクセル数です。
settings-sidebar-row-padding = 行の余白
settings-sidebar-row-padding-description = 行のテキストの上下の余白です。カードレイアウトには影響しません。
settings-sidebar-row-gap = 行内の間隔
settings-sidebar-row-gap-description = 行のステータスドットとラベルの間隔です。
settings-sidebar-host-gap = ホスト内の間隔
settings-sidebar-host-gap-description = ホストヘッダーの各要素の間隔です。
settings-sidebar-host-colours = ホストの色
settings-sidebar-host-colours-description = ホストのヘッダー、ワークスペース、ワークツリー、エージェントに色を付けます。
settings-indicators-edge-rail = 端のバー
settings-indicators-edge-rail-description = 作業中、ブロック中、完了の各行の先頭側の端にバーを表示します。どのアイコンを選んでも表示されます。
settings-indicators-sets = アイコン
settings-indicators-sets-description = 「ドット」「塗りつぶし」「アウトライン」「タイル」「ウェーブ」「点字」「Orca」のステータスアイコンです。作業中のアイコンは常に動きます。
settings-fonts-terminal = ターミナル
settings-fonts-sidebar = サイドバー
settings-fonts-tabs = タブ
settings-fonts-ui = インターフェイス
settings-fonts-all-row = すべての表示箇所
settings-status-usage = 使用量
settings-status-usage-description = サインイン中の AI サービスのプラン使用量を、詳細またはコンパクトに表示します。
settings-status-system-load = CPU とメモリ
settings-status-system-load-description = CPU とメモリを非表示にすると、サイドバーのホスト行からも削除されます。
settings-status-gpu = GPU とビデオメモリ
settings-status-gpu-description = 選択中のホストの GPU 使用率と VRAM です。nvidia-smi、amdgpu ドライバ、または Mac では ioreg から取得します。デフォルトはオフです。
settings-status-network-quality = ネットワークの遅延とパケットロス
settings-status-network-quality-description = このマシンから 15 秒ごとに 1.1.1.1:443 への TCP 接続 1 回の時間を計測します。通信量は 1 日あたり約 1 MB です。
settings-status-keep-awake = スリープさせない
settings-status-keep-awake-description = カップをオンにすると、マシンがスリープしなくなります。「システムのみ」では caffeinate -i と同様に、ディスプレイのスリープとロックだけを許可します。すでにオンのカップは、一度オフにしてオンにし直すまで範囲が変わりません。
settings-status-theme = テーマ
settings-status-shortcuts = ショートカット
settings-status-report-issue = 問題を報告
settings-general-language = 言語
settings-general-language-description = Sheprd の表示言語です。「システム」では、システムの優先する言語の順序に従います。翻訳のないテキストは英語で表示されます。
settings-general-ports = 待ち受けポートを表示
settings-general-ports-description = 開発サーバーがあるワークスペースにポートのチップを表示します。
settings-general-checkpoints = エージェントのターンごとにチェックポイントを作成
settings-general-checkpoints-description = エージェントのターンが終わるたびに、ワークツリーのスナップショットを作成します。
settings-general-smooth-scroll = スムーズスクロール
settings-general-smooth-scroll-description = ターミナルの履歴を行の途中でもスクロールします。オフの場合は行単位で移動します。
settings-general-copy-on-select = 選択時にコピー
settings-general-copy-on-select-description = オフの場合は、⌘C / Ctrl-C で強調表示された選択範囲をコピーします。
settings-general-copied-toast = コピー時の通知
settings-general-clipboard-position = 通知の位置
settings-general-merged-worktrees = マージ済み PR のワークツリーを削除
settings-general-merged-worktrees-description = PR がマージされると、変更のないリンクされたワークツリーを削除し、そのワークスペースを閉じます。フォーカス中のワークスペースでは先に確認します。
settings-general-browser-skill = ローカルのエージェントにブラウザタブの使い方を教える
settings-general-browser-skill-description = ~/.claude と ~/.agents にインストールされ、常に最新の状態に保たれます。
settings-general-launch-at-login = ログイン時に起動
settings-general-launch-at-login-description = Windows にサインインしたときに Sheprd を開きます。
settings-general-update-beta = ベータ版をインストール
settings-general-update-beta-description = ベータ版は安定版に昇格する前にリリースされます。オフにしてもダウングレードはされません。オンの間、Homebrew でのインストールはその場でアップデートされます。
settings-tabs-layout = タブのレイアウト
settings-tabs-layout-description = タブバーでのタブの描画方法です。「View > Tab Layout」からも変更できます。
settings-tabs-agent-title = タブにエージェントのタイトルを表示
settings-tabs-agent-title-description = 名前の付いていないタブでは、番号の代わりにエージェントのターミナルタイトルを表示します。
settings-tabs-herdr-2 = 新しいターミナルで Herdr 2 を使用
settings-tabs-herdr-2-description = このコンピュータの新しいタブは herdr-core で実行され、ネイティブの分割ペインを使えます。Herdr 1 のタブはそのまま並行して動作します。
settings-tabs-position = 位置
settings-tabs-hide-single = タブが 1 つのときは非表示
settings-tabs-confirm-tab = タブを閉じる前に確認
settings-tabs-confirm-pane = ペインを閉じる前に確認
settings-tabs-confirm-workspace = ワークスペースを閉じる前に確認
settings-tabs-undo-close = タブを閉じる操作の取り消し
settings-tabs-undo-close-description = 閉じたターミナルタブは非表示のまま 10 秒間、または [undo_close_tab] の `seconds` で指定した時間だけ実行され続けます。「Reopen Closed Tab」で、プログラムが実行中のまま同じタブを戻せます。それまでの間は、デーモンの他のクライアントにもこのタブが表示されます。
settings-tabs-focus-follows-mouse = フォーカスをマウスに追従
settings-tabs-focus-follows-mouse-description = 分割されたターミナルタブ内で、ポインタのあるペインがアクティブになります。
settings-tabs-herdr-2-scrollback = Herdr 2 のスクロールバック
settings-tabs-herdr-2-scrollback-description = 各 Herdr 2 ターミナルがさかのぼって表示するために保持する出力の行数です。今後開くターミナルから新しいサイズが適用されます。
# $lines is a number of lines of terminal output.
settings-tabs-herdr-2-scrollback-lines = { $lines }
settings-tabs-pane-history = 再起動後に Herdr 1 のスクロールバックを復元
settings-tabs-pane-history-description = 実験的な機能です。Herdr 1 はペインの出力を session-history.json に保存し、デーモンの再起動時に再生します。Herdr 2 では設定は不要です。herdr-core がターミナルを実行し続けるため、上の Herdr 2 のスクロールバックの範囲で出力ごと復元されます。
settings-chat-layout = チャットのレイアウト
settings-chat-layout-description = チャット表示でのメッセージと入力欄の描画方法です。タイムライン、Claude アプリのような吹き出し、Claude Code のターミナル画面、ドキュメント、ターンごとのカード、「メッセージ」のような吹き出し、または iA Writer のような静かな 1 列から選べます。チャットのヘッダーにあるボタンでも順に切り替えられます。
settings-chat-full-width = チャットを全幅で表示
settings-chat-full-width-description = メッセージと入力欄に、利用できる領域をすべて使います。
settings-chat-width = チャットの幅
settings-chat-width-description = 論理ピクセル単位での最大の表示幅です。狭いウインドウでは自動的に収まります。
settings-chat-launcher = チャットランチャー
settings-chat-launcher-description = インストール済みの各エージェントとワンクリックでチャットを始めるボタンを、タブバーのどこに表示するかです。どの配置でも「+」はターミナルタブを開きます。
settings-chat-auto-convert = 新しいエージェントをチャットとして開く
settings-chat-auto-convert-description = ターミナルで起動された Claude、Codex、Copilot、OpenCode のエージェントを（自分が起動したものも他のツールが起動したものも）、数秒間アイドル状態になった時点でチャットに移します。すでに実行中のエージェント、入力中のエージェント、ターミナルに戻したチャットはターミナルのままです。
settings-chat-glide = チャットのスムーズスクロール
settings-chat-glide-description = 返信がストリーミングされるとき、新しいテキストまでスムーズにスクロールします。オフの場合は、チャンクが届くたびにその位置へジャンプします。
settings-chat-wrap-code = コードを折り返す
settings-chat-wrap-code-description = コードブロックの長い行を、横にスクロールさせずに折り返します。各ブロックの「Wrap」ボタンでは、そのブロックだけを変更できます。
settings-chat-code-line-numbers = コードに行番号を表示
settings-chat-code-line-numbers-description = コードブロックの各行の横に行番号を表示し、折り返された行が 1 行として読めるようにします。
settings-chat-code-stripes = コードの行を交互に色分け
settings-chat-code-stripes-description = コードブロックの 1 行おきに少し異なる背景色を付け、折り返された行が 1 行として読めるようにします。
settings-chat-render-html = HTML ブロック
settings-chat-render-html-description = エージェントの HTML ブロックの初期表示です。「Always」はクリックせずにページを読み込み、「Never」はコードを表示し、「Default」はクリックするとページを読み込むカードを表示します。各ブロックのボタンでは、ページとコードを切り替えられます。
settings-code-mode = VS Code サーバー
settings-code-mode-description = VS Code を自動的に起動するか、自分で実行しているサーバーのアドレスを使用します。
settings-notif-delivery = エージェントの通知の配信先
settings-notif-delivery-description = 「Sheprd」ではアプリ内に表示します。「システム」では OS の通知を使い、クリックするとそのペインが開きます。「ターミナル」は他のクライアント専用です。
settings-notif-finished = 完了の通知
settings-notif-finished-description = オフにすると、エージェントのターンごとの完了通知を表示しません。対応が必要なときの通知は引き続き表示されます。
settings-notif-quiet-hours = サイレント時間帯
settings-notif-quiet-hours-description = このコンピュータの時計で毎日決まった時間帯に、サウンド、トースト、システム通知を止めます。バッジと「Overview」は引き続き更新され、止めた通知は「Recent alerts」に残ります。
settings-notif-show = トーストを表示
settings-notif-delay = 遅延
settings-notif-delay-description = トーストを表示するまで待つことで、すぐに終わるターンでは通知しないようにします。
settings-notif-position = 位置
settings-notif-sound = エージェントのサウンドを再生
settings-notif-sound-description = Herdr のサウンドのパスとエージェントごとの上書き設定を使用します。
settings-notif-volume = 音量
settings-notif-volume-description = サウンドと「テスト」はこの音量で再生されます。サウンドがオフのときも同様です。
settings-integrations-search = 連携
settings-integrations-search-description = Herdr が起動・追跡できるエージェントです。
settings-plugins-values = サイドバーの値
settings-plugins-values-description = プラグインが報告する値を、サイドバーの独立した行として表示します。
settings-cloud-accounts = クラウドアカウント
settings-cloud-accounts-description = 各クラウドプロバイダのアカウントと、デバイスとして保存したマシンです。
settings-cloud-setup = 新しいデバイスのセットアップ
settings-cloud-setup-description = 新しいクラウドデバイスが初めて接続したときにインストールする、Claude Code、Codex、Gemini CLI、OpenCode などのエージェントとセットアップスクリプトです。
settings-remote-enabled = リモートコントロール
settings-remote-enabled-description = ユーザーサービスとして、ペアリングしたスマートフォンに HTTPS でチャットを提供します。
settings-remote-keep-running = Sheprd の終了後も実行し続ける
settings-remote-keep-running-description = アプリを終了した後も、スマートフォンは引き続き使用でき、通知も届き続けます。
settings-remote-port = ポート
settings-remote-port-description = スマートフォンが接続する HTTPS ポートです。
settings-remote-bind = 待ち受けアドレス
settings-remote-bind-description = すべてのインターフェイス、または特定のネットワークのアドレスだけで待ち受けます。
settings-remote-addresses = アドレス
settings-remote-addresses-description = スマートフォンがこのコンピュータに接続するアドレスです。LAN と、実行中の場合は Tailscale のアドレスです。
settings-remote-certificate = 証明書
settings-remote-certificate-description = デーモン独自の認証局とそのフィンガープリント、または自分で用意したファイルです。
settings-remote-notifications = 通知のテキスト
settings-remote-notifications-description = スマートフォンの通知に、メッセージの短い抜粋を含めるかどうかです。
settings-remote-idle = 未使用デバイスの登録解除までの期間
settings-remote-idle-description = ペアリングしたデバイスが接続しないままでいられる期間（1 日、1 週間、1 か月、または無期限）です。
settings-remote-pair-card = スマートフォンをペアリング
settings-remote-pair-card-description = スマートフォンで読み取る QR コードを表示してから、承認します。
settings-remote-devices = ペアリング済みのデバイス
settings-remote-devices-description = チャットを操作できるデバイスと、それぞれの最終接続日時です。ここでアクセスを取り消せます。
settings-config-local = Sheprd の設定
settings-config-local-description = このマシンで Sheprd 独自の設定を保存しているファイルです。
settings-config-shared = Herdr の構成
settings-config-shared-description = Herdr の構成ファイルです。Herdr のターミナルクライアントもこのファイルを読み込みます。
settings-config-reload = 構成を再読み込み
settings-config-reload-description = 両方のファイルを読み込み直します。デーモンは再起動しません。
