using UnityEngine;

namespace Core.Commands
{
    public class DeleteObjectCommand : ICommand
    {
        private readonly GameObject _targetObject;

        public DeleteObjectCommand(GameObject targetObject)
        {
            _targetObject = targetObject;
        }

        public void Execute()
        {
            if (_targetObject != null)
            {
                _targetObject.SetActive(false);
            }
        }

        public void Undo()
        {
            if (_targetObject != null)
            {
                _targetObject.SetActive(true);
            }
        }
    }
}
