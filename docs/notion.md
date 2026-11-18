# Notion search in Katran

Install the official Notion CLI and check that `ntn --version` works from
Windows. Katran also detects WinGet installations directly when its launcher
still has an older PATH.

1. Create an [internal Notion connection](https://developers.notion.com/guides/connections-and-tokens/internal-connections)
   with **Read content** enabled. Leave **Update content** and **Insert content**
   disabled.
2. In Notion, use **Connections** to grant access to two test pages. A
   connection on a parent page can also grant access to its descendants.
3. In Katran, open **Settings → Integrations → Notion**, enter the integration's
   token and save it. Katran keeps it in Windows Credential Manager for the
   current Windows user. Disconnect removes that local credential.
4. From the app that should receive the text, summon Katran with its overlay
   hotkey. Open **Ctrl+Shift+K → Notion** and type a page title. Use **↓** or
   click a result to focus the list. **Space** opens or closes its Markdown
   preview. **Enter** fetches the Markdown and pastes it into the previous
   window using Katran's usual clipboard and overlay guards.

Search requests match page titles and return up to 25 items. Type a more
specific title if there are additional matches. Katran does not index this
content locally. It debounces title changes by 300 ms, cancels stale jobs
and bounds each CLI request to eight seconds. Preview renders supported
Markdown as text without loading linked images. Katran rejects truncated
Notion responses. The preview limit is 65,536 characters and the paste
limit is 1 MiB of Markdown. Notion data sources, favorites and history are
outside this pilot.
