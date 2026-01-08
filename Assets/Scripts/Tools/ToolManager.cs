using System.Collections.Generic;
using UnityEngine;
using UnityEngine.EventSystems;

namespace Tools
{
    public class ToolManager : MonoBehaviour
    {
        public static ToolManager Instance { get; private set; }

        public Tool CurrentTool { get; private set; }

        private List<Tool> _availableTools = new List<Tool>();

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

        private void Update()
        {
            if (CurrentTool != null && !EventSystem.current.IsPointerOverGameObject())
            {
                CurrentTool.OnToolUpdate();
            }
        }

        public void RegisterTool(Tool tool)
        {
            if (!_availableTools.Contains(tool))
            {
                _availableTools.Add(tool);
            }
        }

        public void SelectTool(Tool tool)
        {
            if (CurrentTool != null)
            {
                CurrentTool.OnToolDeselected();
            }

            CurrentTool = tool;

            if (CurrentTool != null)
            {
                CurrentTool.OnToolSelected();
                Debug.Log($"Tool Selected: {CurrentTool.ToolName}");
            }
        }

        public void SelectTool(string toolName)
        {
            var tool = _availableTools.Find(t => t.ToolName == toolName);
            if (tool != null)
            {
                SelectTool(tool);
            }
            else
            {
                Debug.LogWarning($"Tool not found: {toolName}");
            }
        }

        public T GetTool<T>() where T : Tool
        {
             foreach(var tool in _availableTools)
             {
                 if (tool is T t) return t;
             }
             return null;
        }
    }
}
