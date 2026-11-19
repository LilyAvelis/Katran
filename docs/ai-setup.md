# Katran 4.0: установка AI на другом компьютере

Обычная полка Katran работает без моделей. Через **Settings → AI** можно
установить поиск эмодзи, нейропоиск папок, перевод и увеличение изображений.
Весов в установщике Katran нет. Для первой установки нужен интернет.

## Translate и Upscale

Нажми **Install** у нужной функции. Терминал скачает недостающие компоненты
в профиль Katran и выполнит пробный расчёт. **Verify** повторяет расчёт с
установленными моделями без загрузки весов. Уже настроенные ComfyUI и Ollama
можно использовать вместе с Katran, их окружения установщик не обновляет.
В новом профиле Upscale получает CPU-версию PyTorch. Совместимый Python из
существующей ComfyUI может выполнять Upscale на CUDA.

Нужны Windows x64, свободное место и интернет. Translate скачивает несколько
гигабайт. Установку и Verify можно запускать повторно. Сохранившиеся целые
файлы используются снова.
Translate принимает японский, упрощённый и традиционный китайский, корейский;
результат — английский. Для Qwen требуется полное размещение модели на GPU;
одного обнаружения файлов или NVIDIA недостаточно, чтобы подтвердить работу.

Ручной маршрут ниже нужен для отдельного расположения окружений и весов.
При передаче сборки на другой компьютер сначала проверь **Install**, затем
**Verify** и одну обычную операцию с небольшим изображением. Это особенно
важно для Translate, где результат зависит от драйвера и доступной видеопамяти.

Профиль установленной версии обычно `%LOCALAPPDATA%\Katran\data`, portable —
`katran-data` рядом с приложением. Далее он обозначен `<profile>`; это не папка
исходного кода. Все пути и имена файлов ниже должны совпадать точно.

### Общий portable Python

Установи [ComfyUI Windows Portable](https://github.com/Comfy-Org/ComfyUI)
в `%SystemDrive%\ComfyUI\ComfyUI_windows_portable`, либо задай абсолютный
`KATRAN_COMFYUI_ROOT` в окружении, из которого запускается Katran. В корне
должен существовать `python_embeded\python.exe`. Сам сервер ComfyUI запускать
не требуется. Katran использует этот Python для Upscale и части Translate,
в том числе при переводе китайского и корейского.

В рабочем локальном окружении проверялись Python 3.13.11, PyTorch 2.10.0+cu130,
Pillow 12.1.1, NumPy 2.4.2, OpenCV 4.13.0.92, `spandrel` 0.4.2 и
`transformers` 5.2.0. Наличие этих версий не гарантирует совместимости другого
выпуска ComfyUI. Проверь импорты его Python; при отсутствии пакета установи его
в отдельной копии окружения. Не обновляй вслепую PyTorch/CUDA в уже работающей
ComfyUI: совместимость зависит от драйвера и сборки PyTorch.

```powershell
& "$env:SystemDrive\ComfyUI\ComfyUI_windows_portable\python_embeded\python.exe" -c "import torch, PIL, numpy, cv2, spandrel, transformers; print(torch.__version__, torch.cuda.is_available())"
```

Если выбран другой корень, подставь его в команду. Успешный импорт — только
проверка окружения, не проверка inference или доступности GPU.

### Upscale

Положи нужные файлы в `<profile>\ai-models\upscale_models` или в
`<ComfyUI root>\ComfyUI\models\upscale_models`. Поддерживаются также пути из
`extra_model_paths.yaml` ComfyUI. Каждая команда требует свои веса; остальные
можно не скачивать. Сверь SHA256 через `Get-FileHash <путь> -Algorithm SHA256`.

| Файл | Источник | SHA256 проверенного файла |
| --- | --- | --- |
| `RealESRGAN_x2plus.pth` | [веса из релиза Real-ESRGAN v0.2.1](https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.1/RealESRGAN_x2plus.pth) | `49fafd45f8fd7aa8d31ab2a22d14d91b536c34494a5cfe31eb5d89c2fa266abb` |
| `RealESRGAN_x4.pth` | [ai-forever Real-ESRGAN](https://huggingface.co/ai-forever/Real-ESRGAN/blob/main/RealESRGAN_x4.pth) | `aa00f09ad753d88576b21ed977e97d634976377031b178acc3b5b238df463400` |
| `4x_foolhardy_Remacri.pth` | [зеркало ESRGAN](https://huggingface.co/JCTN/ESRGAN/blob/main/4x_foolhardy_Remacri.pth) | `e1a73bd89c2da1ae494774746398689048b5a892bd9653e146713f9df8bca86a` |

Последняя ссылка — стороннее зеркало, не заявленный официальный источник.
`RealESRGAN_x4.pth` и `RealESRGAN_x4plus.pth` — разные веса; не переименовывай
одни в другие. Команда ×2 с Remacri использует модель ×4 и уменьшает результат
до точного размера ×2. При отсутствии CUDA Upscale может использовать CPU.

### Translate

1. Установи отдельный Python 3.10 x64 с [python.org](https://www.python.org/downloads/)
   и создай виртуальное окружение `<profile>\translate-runtime\venv`. В локальной
   проверке использовались Python 3.10.11, `paddlepaddle==3.2.2`,
   `paddleocr==3.3.3`, `paddlex==3.3.13`. Например, в PowerShell, заменив путь:

   ```powershell
   $runtime = 'C:\path\to\katran-data\translate-runtime'
   py -3.10 -m venv "$runtime\venv"
   & "$runtime\venv\Scripts\python.exe" -m pip install paddlepaddle==3.2.2 paddleocr==3.3.3 paddlex==3.3.13
   ```

   Это отдельное CPU-окружение OCR, не Python ComfyUI. Если `py -3.10` не
   найден, укажи полный путь к установленному Python 3.10. Разрешение свежих
   зависимостей pip и установка на другой машине ещё не проверены целиком.
2. Скачай официальные архивы Paddle: [детектор](https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_server_det_infer.tar),
   [китайский распознаватель](https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv5_server_rec_infer.tar),
   [корейский распознаватель](https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/korean_PP-OCRv5_mobile_rec_infer.tar).
   Ссылки приведены в документации [Paddle OCR](https://www.paddleocr.ai/latest/en/version3.x/pipeline_usage/OCR.html)
   и [распознавания текста](https://www.paddleocr.ai/latest/en/version3.x/module_usage/text_recognition.html).
   Распакуй каждый отдельно, затем размести содержимое в соответствующих
   каталогах так, чтобы файлы лежали непосредственно в них:

   ```text
   <profile>\translate-runtime\paddlex\official_models\
     PP-OCRv5_server_det\inference.json, inference.pdiparams, inference.yml
     PP-OCRv5_server_rec\inference.json, inference.pdiparams, inference.yml
     korean_PP-OCRv5_mobile_rec\inference.json, inference.pdiparams, inference.yml
   ```

   Детектор нужен для всех языков, китайская модель — для обоих вариантов
   китайского, корейская — для корейского. Если архив содержит дополнительный
   вложенный каталог, перенеси файлы из него, не оставляя лишнего уровня.
3. Для японского скачай файлы [manga-ocr-base, фиксированная ревизия](https://huggingface.co/kha-white/manga-ocr-base/tree/aa6573bd10b0d446cbf622e29c3e084914df9741)
   в `<profile>\translate-runtime\manga-model`: `config.json`,
   `preprocessor_config.json`, `pytorch_model.bin`, `vocab.txt`,
   `special_tokens_map.json`, `tokenizer_config.json`. Файлы должны лежать
   непосредственно в этом каталоге. Katran загружает локальные веса без
   выполнения удалённого кода.
4. Установи [Ollama для Windows](https://ollama.com/download/windows), затем
   самостоятельно выполни `ollama pull qwen3:4b-instruct` и проверь `ollama list`.
   Нужен именно [этот тег](https://ollama.com/library/qwen3:4b-instruct), а не
   `qwen3:4b` или облачная модель. Katran ищет Ollama в стандартном каталоге
   установки или PATH; альтернативный абсолютный путь задаёт
   `KATRAN_OLLAMA_EXE`. Модели ищутся сначала в `<profile>\translate-ollama`,
   затем в абсолютном `OLLAMA_MODELS`, затем в `%USERPROFILE%\.ollama\models`.
   При нестандартном пути задай переменную до запуска Katran. Наличие модели в
   списке не подтверждает её полное размещение на GPU: это проверяется при работе.

Вместо `<profile>\translate-runtime` можно использовать абсолютный
`KATRAN_TRANSLATE_ROOT`; структура внутри остаётся такой же. Переменные должны
быть доступны именно процессу Katran: изменения в уже открытом PowerShell не
передаются автоматически ранее запущенному приложению или другому launcher.
Строки с именами языков показывают наличие компонентов. Результат кнопки
**Verify** выводится отдельно и включает пробный OCR и перевод через Qwen.
Подробнее о требованиях и локальной проверке — [Translate](patch-4.0/TRANSLATE.md).

## Поиск эмодзи и папок: быстрый старт

1. Установи Katran из официального setup или распакуй portable в отдельную папку
   и запусти `Katran.exe`. Не копируй папку разработки или чужой профиль.
2. Открой **Settings → AI** и нажми **Install** у нужного компонента:
   **Emoji search** для эмодзи или **Folder neural search** для папок.
3. Появится отдельное окно терминала. Дождись результата, сохрани путь журнала
   при ошибке. Терминал остаётся открытым, чтобы его можно было прочитать.
4. Нажми **Verify** для функциональной проверки установленного компонента.
   Наличие файлов само по себе ещё не подтверждает работу модели.
5. Для папок открой выбор папки (Ctrl+Shift+K), нажми физический обратный слеш
   для нейрорежима, введи запрос и нажми Enter. Для получения результатов
   нужен доступный список кандидатов папок; обычный поиск использует Everything.

Установка использует отдельные окружения в профиле Katran. При отсутствии
подходящего Python помощник может установить приватный Python 3.10 x64,
проверяя подпись официального установщика. Системный PATH не изменяется.
Оба поисковых компонента используют multilingual-e5-small, но имеют отдельные
окружения. Можно установить только один. Это локальный поиск, не чат-модель.

## Ускорение Folder на NVIDIA

Сначала установи и проверь компонент поиска папок, затем дополнительный
**CUDA** в том же разделе. После успешной проверки включи GPU для Folder.
Emoji search в этой версии использует CPU; CUDA-карточка ускоряет Folder.

GeForce RTX 4070 / 4070 SUPER поддерживают CUDA. Нужен совместимый NVIDIA
драйвер; одного названия видеокарты недостаточно для подтверждения установки.
Скрипт Katran ставит изолированные библиотеки CUDA 12.9/cuDNN 9 и ONNX Runtime,
не заменяя системный CUDA Toolkit или драйвер. Полный Toolkit вручную обычно
не требуется: необходимые runtime-библиотеки устанавливает компонент.
Проверка запускает настоящий расчёт и проверяет выполнение CUDA-узлов.

Если CUDA не прошла проверку, выключи GPU в Settings → AI и используй CPU.
Не нужно переустанавливать сам Katran или удалять полку. Источники требований:
[NVIDIA](https://docs.nvidia.com/deploy/pdf/CUDA_Compatibility.pdf),
[ONNX Runtime CUDA](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html).

## Если установка не удалась: передай своему ИИ

В терминале показаны путь `install.log` и команда для чтения журнала. Рядом
сохраняются `request.json` и `result.json`. Для setup профиль обычно находится
в `%LOCALAPPDATA%\Katran\data`, у portable — `katran-data` рядом с приложением.
Журналы установки лежат внутри `ai-installations/<id>` этого профиля.
Проверь журнал перед отправкой: он может содержать локальные пути.

Сообщи помощнику версию Katran, нужный компонент, точное название GPU/версию
драйвера и ошибку из журнала. Попроси проверить этот журнал и встроенные
version-matched установочные скрипты, зависимости и доступность загрузок;
исправлять изолированное окружение конкретного компонента. Не удалять данные
полки, не менять глобальные Python/CUDA/PATH без объяснения необходимости и
не считать установку успешной до **Verify**. Для GPU успех означает настоящий
расчёт на CUDA, а не только присутствие провайдера в списке.

Для обычной установки код репозитория скачивать не нужно: скрипты встроены в
приложение; полный portable также содержит их в `scripts/ai-components`,
`scripts/emoji-ai` и `scripts/folder-semantic`. Локальный помощник может изучить
и исправить материализованную копию скрипта вручную. Изменённая копия не
выдаётся приложением за исходную проверенную версию.

Установка с нуля на другом ПК ещё требует практической проверки. На компьютере
разработки проверены CPU-компоненты и реальный CUDA worker; это не обещание
совместимости со всеми конфигурациями Windows.
