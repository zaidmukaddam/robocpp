# Vendor PLCopen Fixtures

Place real vendor PLCopen XML exports here when licensing permits them to be
committed. Each XML fixture must include a `validation:` metadata comment.

Representative public/export sources to use during release validation:

| Source | Expected use | Notes |
| --- | --- | --- |
| CODESYS PLCopenXML export | Import/export behavior and CODESYS-specific PLCopen limitations. | CODESYS documents Project -> Export PLCopenXML and warns that 100% compatibility is not guaranteed for every PLCopen subset. |
| TwinCAT-origin XML/project files | XML comparison and TwinCAT dialect checks. | Beckhoff documents XML comparison as part of TwinCAT project comparison. |
| OpenPLC PLCopen XML examples | Open-source XML import smoke fixtures. | Public examples should be committed only when their license permits redistribution. |
| Beremiz PLCopen XML libraries | Open-source PLCopen 2.01 fixtures. | Commit only license-compatible minimized fixtures. |

Checked-in third-party fixtures must keep source URL, license, copyright, and
local modification notes in `THIRD_PARTY_NOTICES.md`.

Record accepted extensions and rejected dialect features in `DIALECTS.md`.
