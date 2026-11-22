# Settings > Remote control: the daemon's switch and how it serves, pairing
# a phone, and the paired devices. Also the pane history switch Settings
# saves on each SSH host.

settings-remote-health-updating = サービスを更新中…
settings-remote-health-off = オフ。オンにすると、ネットワーク上で HTTPS 経由で（Tailscale の実行中は Tailscale 経由でも、「リレー」でリレーを追加した後はリレー経由でも）チャットを提供するユーザーサービスがインストールされます。
settings-remote-health-starting = 起動中…
settings-remote-health-running = ポート { $port } で実行中 · デバイス { $devices } 台、{ $connected } 台が接続中
settings-remote-health-running-alone = ポート { $port } で実行中 · デバイス { $devices } 台、{ $connected } 台が接続中 · Herdr は実行されていません
settings-remote-health-not-running = 実行されていません：{ $reason }
settings-remote-service-failed = サービスを変更できませんでした：{ $error }
settings-remote-revoke-failed = デバイスのアクセスを取り消せませんでした：{ $error }
settings-remote-retry = 再試行
# Shown while the service runs a build without a developer ID and the
# macOS firewall is on.
settings-remote-firewall = このビルドが変わるとスマートフォンがブロックされます
settings-remote-firewall-note = macOS のファイアウォールがオンで、このデーモンのビルドは Developer ID で署名されていないため、macOS はこのビルドだけを許可します。再ビルドすると、次に Mac またはデーモンを再起動したとき、このコンピュータで誰かが「許可」をクリックするまでスマートフォンがブロックされます。許可を維持するには、署名済みのリリース版でリモートコントロールを実行してください。

settings-remote-section-service = サービス
settings-remote-keep-running-note = スマートフォンは引き続き使え、通知も届きます。新しいデバイスのペアリングには引き続き Sheprd が必要です。
settings-remote-presence = 在席中はスマートフォンへの通知を保留
settings-remote-presence-note = Bluetooth でこのコンピュータの近くにあるペアリング済みの iPhone は、あなたがコンピュータから離れるまで通知を保留します。
settings-remote-restart = デーモンを再起動
settings-remote-restart-note = アップデート後と同じように、Sheprd と同じ場所にあるバイナリから起動し直します。スマートフォンは自動的に再接続します。
settings-remote-restart-button = 再起動

settings-remote-section-connection = 接続
settings-remote-bind-all = すべてのインターフェイス
settings-remote-addresses-none = ネットワークアドレスが見つかりません。
settings-remote-while-running = リモートコントロールの実行中に表示されます。
settings-remote-certificate-own = 独自の CA
settings-remote-certificate-files = 自分のファイル
settings-remote-certificate-files-note = 独自ドメイン用など、自分で用意した証明書と鍵。
settings-remote-certificate-own-note = デーモン独自の認証局です。スマートフォンで一度信頼すれば使えます。フィンガープリントは実行中に表示されます。
settings-remote-certificate-fingerprint = デーモン独自の認証局です。スマートフォンで一度信頼すれば使えます。CA SHA-256 { $fingerprint }
settings-remote-files = 証明書ファイル
settings-remote-files-certificate = 証明書
settings-remote-files-key = 秘密鍵
settings-remote-files-invalid = 両方のファイルを絶対パスで指定してください。
settings-remote-files-apply = これらのファイルを使用

settings-remote-section-relays = リレー
settings-remote-relays = どのネットワークからでもこのコンピュータに接続
settings-remote-relays-none = リレーなし。スマートフォンは同じネットワーク上から、または Tailscale の実行中は Tailscale 経由でこのコンピュータに接続します。
settings-remote-relays-note = ペアリング済みのスマートフォンは、直接の経路がない場合にリレーを経由します。リレーから見えるのは暗号化された通信だけです。新しいリレーは、スマートフォンが次に直接接続したときに伝わります。
settings-remote-relay-listening = 待ち受け中
settings-remote-relay-listening-rtt = 待ち受け中 · { $ms } ms
settings-remote-relay-connecting = 接続中…
settings-remote-relay-failed = 失敗：{ $error }
settings-remote-relay-unreported = デーモンからまだ報告がありません。
# $tags is the relay's tags from remote.relays, joined with commas.
settings-remote-relay-tags = タグ：{ $tags }
settings-remote-relay-remove = 削除
settings-remote-relay-remove-label = { $relay } を削除
settings-remote-relay-add = リレーを追加
settings-remote-relay-add-note = 自分で運用しているリレーのアドレス。追加または削除するとデーモンが再起動します。
settings-remote-relay-add-button = 追加
settings-remote-public-relays = Sheprd のリレー
settings-remote-public-relays-loading = リストを読み込み中…
settings-remote-public-relays-none = 現在利用できる Sheprd のリレーはありません。どこからでもこのコンピュータに接続するには、自分でリレーを運用してください。
settings-remote-public-relays-setup = リレーの運用方法
settings-remote-public-relays-validated = { $signer } により検証済み。追加すると、そのリレーで待ち受けます。
settings-remote-public-relay-named = { $name }（{ $region }）
settings-remote-public-relay-added = 追加済み
settings-remote-public-relay-add-label = { $relay } を追加
settings-remote-relay-invalid = リレーのアドレスではありません。{ $example } のような形式で指定してください。

settings-remote-section-notifications = 通知
settings-remote-notifications-note = 「概要のみ」は対応が必要なチャットを本文なしで知らせ、「抜粋」はメッセージの短い抜粋も加えます。
settings-remote-privacy-generic = 概要のみ
settings-remote-privacy-excerpt = 抜粋
settings-remote-live-activities = ライブアクティビティ
settings-remote-live-activities-note = 「詳細」は、ペアリング済みの iPhone のロック画面にチャットと現在のステップを表示します。質問と回答ボタンの表示には「抜粋」も必要です。「概要のみ」は、エージェントとその大まかな作業内容だけを表示します。スマートフォン側の設定で、表示を減らすこともできます。
settings-remote-live-activities-detailed = 詳細
settings-remote-live-activities-generic = 概要のみ

settings-remote-section-devices = デバイス
settings-remote-idle-note = この期間接続していないデバイスは登録が解除され、再度ペアリングが必要になります。
settings-remote-idle-day = 1 日
settings-remote-idle-week = 1 週間
settings-remote-idle-month = 1 か月
settings-remote-idle-never = 無期限
settings-remote-devices-none = ペアリング済みのデバイスはまだありません。
settings-remote-devices-count = { $count } 台のデバイスがチャットを操作できます。
settings-remote-device-revoke-ask = このデバイスのセッションは終了し、通知もすぐに停止します（ここと、ここからペアリングしたマシンの両方で）。アクセスを取り消しますか？
settings-remote-device-cancel = キャンセル
settings-remote-device-revoke = アクセスを取り消す
settings-remote-device-revoke-open = アクセスを取り消す…
settings-remote-device-revoke-confirm = 取り消しを確定

settings-remote-pair-off = リモートコントロールをオンにしてから、スマートフォンのカメラでコードをスキャンしてください。
settings-remote-pair-ready = スマートフォンのカメラでコードをスキャンしてから、ここでそのスマートフォンを承認してください。
settings-remote-pair-connected = { $count } 台のスマートフォンが接続中です。追加するには、別のスマートフォンのカメラでコードをスキャンしてください。
settings-remote-pair-button = スマートフォンをペアリング…
settings-remote-pair-no-window = デバイスをペアリングするには、Sheprd のウインドウを開いてください。
settings-remote-pair-dialog-open = 開いているダイアログを閉じてから、デバイスをペアリングしてください。

settings-remote-history-reload-failed = 保存しましたが、ホストのデーモンの再読み込みをキューに追加できませんでした：{ $error }

## A private relay's listen token.

settings-remote-relay-token-button = トークン
# $relay is a relay's name or address.
settings-remote-relay-token-label = { $relay } の待ち受けトークンを設定
# $relay is a relay's address.
settings-remote-relay-token = { $relay } の待ち受けトークン
settings-remote-relay-token-note = このリレーはプライベートで、待ち受けトークンを持つコンピュータだけを受け入れます。運用者から受け取ったトークンをペーストしてください。トークンはこのコンピュータと、Sheprd がリレーを共有するコンピュータにだけ保存され、構成ファイルには保存されません。
settings-remote-relay-token-placeholder = 待ち受けトークン
settings-remote-relay-token-invalid = 待ち受けトークンは 16〜255 文字で、スペースは使えません。
settings-remote-relay-token-save = 保存
settings-remote-relay-token-forget = 消去
settings-remote-relay-token-cancel = キャンセル
settings-remote-relay-linked = リンクからプライベートリレーを追加しますか？
# $relay is a relay's address, from the link.
settings-remote-relay-linked-note = { $relay }。待ち受けトークンはリンクに含まれており、Sheprd はこのコンピュータに保存します。このリレーを自分でデプロイした場合か、リンクの送信者を信頼できる場合にだけ追加してください。
settings-remote-relay-linked-dismiss = 閉じる
