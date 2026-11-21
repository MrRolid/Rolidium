# Privacy: when Rolidium uses the network

Rolidium works without the network. It has **two optional functions** that connect to `rolidium.com`, and nothing else does. Everything else (modelling, files, `rolidium cli`,
`rolidium mcp`, the self test, the benchmark, saving diagnostics to a file) never touches the network. All addresses are in one place in the code, `crates/rolidium-net/src/lib.rs`: only `https`
and only the domain `rolidium.com`, the certificate is always checked, a redirect is never followed, and every call has a time limit.

## 1. The question about a new version

*When.* Only if you said **Yes** on the small card shown at the first start (or switched *Check for a new version once a day* on in Settings). Until you answer, the program does not connect to the network.
At most once a day, in the background, with a limit of 5 seconds; the start of the program never waits for it and a network error is not shown to you (it goes to the log).

*What is sent.* One `GET` request:

```
GET https://rolidium.com/api/version.json?v=0.2.0&os=linux&osv=debian%2013&arch=x86_64
User-Agent: Rolidium/0.2.0
```

* `v`: the version of the program; `os`: `linux` or `windows`; `osv`: the name and the **main number** of the version of the system (`debian 13`, `windows 10`), nothing more detailed; `arch`: the processor type.
* Nothing else: no identifier of you or of the computer, no cookies, no data about the hardware, no file names. (Like any web request, it reaches the server with the IP address of your connection, which the
  server sees; the program adds nothing to it.) Settings shows the exact text of the question.

*The answer.* A JSON object with `latest` (a version) and optionally `released` (a date). It is limited to 16 kB and read strictly: a damaged or too long answer is only written to the log. If `latest` is higher
than the running version, a small notice says *Version X is available* with the buttons **Download** and **Skip this version**. *Download* opens the fixed page `https://rolidium.com/download` in your browser; the
address is never taken from the answer of the server. The program downloads and starts nothing by itself.

*How to switch it off.* Answer **No** on the card, or switch it off in Settings. `ROLIDIUM_OFFLINE=1` in the environment switches off all network use of the program whatever the settings say.

## 2. Sending diagnostics

*When.* Only when you ask: the button **Send diagnostics to the publisher** in Settings, or the offer shown at the next start after the program did not end properly (you can say *Not now*). Never automatically.

*What you see first.* The list of the files in the zip (each can be opened), a field for a note (up to 2000 characters) and an optional e-mail address for a reply. **Nothing is sent before you press Send.**

*What is in the zip.* The log of the program, its settings (no folders, no file names, no recent files), data about the graphics adapters and the system, the version of the program and, when the size allows,
small test pictures drawn by the graphics backends (a grid, axes and a box, not your model). **No models and no names of models.** The home folder is replaced by `~` and the names of model files
(`.rolidium`, `.step`, `.stp`, `.stl`) by `<model file>`; your user name and the name of the computer are replaced too. The zip is at most 2 MB.

*How it is sent.* A `POST` (multipart) to `https://rolidium.com/api/diagnostics` with the fields `file` (the zip), `note`, `contact` (only if you typed one), `v` (version) and `os`. The answer carries a report number, which
is shown to you so that you can mention it in the forum.

*If the server does not answer or refuses* (the server may not exist yet), the window says so and offers to save the zip to a file and to send it by e-mail to connect@rolid.com.

You can also save the same zip to a file at any time (*About* or *Settings*, **Save diagnostics**) and send it yourself.

## Links

Web https://rolidium.com, support forum https://rolidium.com/forum, download https://rolidium.com/download, this notice https://rolidium.com/privacy. The About window and the README link to the web and
the forum; they open in the browser of the system when you click them.
