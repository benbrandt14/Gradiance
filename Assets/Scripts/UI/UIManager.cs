using Core;
using Tools;
using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.UI;

namespace UI
{
    public class UIManager : MonoBehaviour
    {
        public static UIManager? Instance { get; private set; }

        private GameObject? _canvas;
        private GameObject? _toolbarPanel;
        private ContextMenuController? _contextMenu;

        private static void ResetScene()
        {
            var rigidbodies = UnityEngine.Object.FindObjectsOfType<Rigidbody2D>();
            foreach (var rb in rigidbodies)
            {
                Destroy(rb.gameObject);
            }
        }

        private void Awake()
        {
            if (Instance == null)
            {
                Instance = this;
            }
            else
            {
                Destroy(gameObject);
            }
        }

        private void Start()
        {
            CreateCanvas();
            CreateToolbar();
            CreateContextMenu();
        }

        private void CreateCanvas()
        {
            var canvasGo = new GameObject("Canvas");
            canvasGo.layer = 5; // UI layer
            var canvas = canvasGo.AddComponent<Canvas>();
            canvas.renderMode = RenderMode.ScreenSpaceOverlay;
            canvas.sortingOrder = 100;

            var scaler = canvasGo.AddComponent<CanvasScaler>();
            scaler.uiScaleMode = CanvasScaler.ScaleMode.ScaleWithScreenSize;
            scaler.referenceResolution = new Vector2(1920, 1080);

            canvasGo.AddComponent<GraphicRaycaster>();

            _canvas = canvasGo;
        }

        private void CreateToolbar()
        {
            _toolbarPanel = new GameObject("Toolbar");
            if (_canvas != null)
            {
                _toolbarPanel.transform.SetParent(_canvas.transform, false);
            }

            var rect = _toolbarPanel.AddComponent<RectTransform>();
            rect.anchorMin = new Vector2(0, 1);
            rect.anchorMax = new Vector2(1, 1);
            rect.pivot = new Vector2(0.5f, 1);
            rect.anchoredPosition = new Vector2(0, 0);
            rect.sizeDelta = new Vector2(0, 60); // Height 60

            var img = _toolbarPanel.AddComponent<Image>();
            img.color = new Color(0.15f, 0.15f, 0.15f, 0.9f);

            var layout = _toolbarPanel.AddComponent<HorizontalLayoutGroup>();
            layout.childControlWidth = false;
            layout.childForceExpandWidth = false;
            layout.spacing = 10;
            layout.padding = new RectOffset(10, 10, 5, 5);
            layout.childAlignment = TextAnchor.MiddleLeft;

            // Add Buttons
            CreateToolButton("Move", () => ToolManager.Instance!.SelectTool("Move"));
            CreateToolButton("Box", () => ToolManager.Instance!.SelectTool("Box"));
            CreateToolButton("Circle", () => ToolManager.Instance!.SelectTool("Circle"));
            CreateToolButton("Hinge", () => ToolManager.Instance!.SelectTool("Hinge"));
            CreateToolButton("Spring", () => ToolManager.Instance!.SelectTool("Spring"));

            CreateSpacer();

            CreateButton("Play/Pause", () => SimulationManager.Instance!.TogglePause());
            CreateButton("Clear All", () => ResetScene());
        }

        private void CreateToolButton(string name, UnityEngine.Events.UnityAction action)
        {
            CreateButton(name, action);
        }

        private void CreateButton(string text, UnityEngine.Events.UnityAction action)
        {
            var btnGo = new GameObject(text + "Button");
            if (_toolbarPanel != null)
            {
                btnGo.transform.SetParent(_toolbarPanel.transform, false);
            }

            var img = btnGo.AddComponent<Image>();
            img.color = new Color(0.3f, 0.3f, 0.3f);

            var btn = btnGo.AddComponent<Button>();
            btn.onClick.AddListener(action);

            var layout = btnGo.AddComponent<LayoutElement>();
            layout.minWidth = 100;
            layout.minHeight = 50;

            var textGo = new GameObject("Text");
            textGo.transform.SetParent(btnGo.transform, false);
            var textComp = textGo.AddComponent<Text>();
            textComp.text = text;

            var font = Resources.GetBuiltinResource<Font>("Arial.ttf");
            if (font == null)
            {
                font = Font.CreateDynamicFontFromOSFont("Arial", 14);
            }

            textComp.font = font;

            textComp.alignment = TextAnchor.MiddleCenter;
            textComp.color = Color.white;
            textComp.resizeTextForBestFit = true;

            var textRect = textGo.GetComponent<RectTransform>();
            textRect.anchorMin = Vector2.zero;
            textRect.anchorMax = Vector2.one;
            textRect.offsetMin = Vector2.zero;
            textRect.offsetMax = Vector2.zero;
        }

        private void CreateSpacer()
        {
            var spacer = new GameObject("Spacer");
            if (_toolbarPanel != null)
            {
                spacer.transform.SetParent(_toolbarPanel.transform, false);
            }

            spacer.AddComponent<LayoutElement>().minWidth = 50;
        }

        private void CreateContextMenu()
        {
            var menuGo = new GameObject("ContextMenu");
            if (_canvas != null)
            {
                menuGo.transform.SetParent(_canvas.transform, false);
            }

            _contextMenu = menuGo.AddComponent<ContextMenuController>();
            menuGo.SetActive(false); // Start hidden
        }
    }
}
