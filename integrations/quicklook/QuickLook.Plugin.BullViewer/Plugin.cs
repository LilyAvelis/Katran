using QuickLook.Common.Plugin;
using System;
using System.IO;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;

namespace QuickLook.Plugin.BullViewer
{
    public sealed class Plugin : IViewer
    {
        private BullDocument document;
        private ContextObject context;
        private Image image;
        private TextBlock status;
        private Button previous, next;
        private int page, generation;
        private bool closed;
        public int Priority { get { return 1; } }
        public void Init() { }
        public bool CanHandle(string path)
        {
            if (!string.Equals(Path.GetExtension(path), ".bull", StringComparison.OrdinalIgnoreCase)) return false;
            try
            {
                using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
                {
                    byte[] magic = new byte[8];
                    return file.Read(magic, 0, magic.Length) == 8 && System.Text.Encoding.ASCII.GetString(magic) == "BALOHCOL";
                }
            }
            catch (IOException) { return false; }
            catch (UnauthorizedAccessException) { return false; }
        }
        public void Prepare(string path, ContextObject value) { value.PreferredSize = new Size(900, 720); }
        public async void View(string path, ContextObject value)
        {
            context = value; closed = false; int token = ++generation;
            context.Title = Path.GetFileName(path); context.IsBusy = true;
            var grid = new Grid(); grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); grid.RowDefinitions.Add(new RowDefinition());
            var toolbar = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Center, Margin = new Thickness(8) };
            previous = new Button { Content = "← Previous", Margin = new Thickness(4), Padding = new Thickness(10, 4, 10, 4), IsEnabled = false };
            next = new Button { Content = "Next →", Margin = new Thickness(4), Padding = new Thickness(10, 4, 10, 4), IsEnabled = false };
            status = new TextBlock { Text = "Loading…", VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(8) };
            previous.Click += (s, e) => ShowPage(page - 1); next.Click += (s, e) => ShowPage(page + 1);
            toolbar.Children.Add(previous); toolbar.Children.Add(status); toolbar.Children.Add(next); grid.Children.Add(toolbar);
            image = new Image { Stretch = Stretch.Uniform, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Top };
            var scroll = new ScrollViewer { HorizontalScrollBarVisibility = ScrollBarVisibility.Auto, VerticalScrollBarVisibility = ScrollBarVisibility.Auto, Content = image };
            Grid.SetRow(scroll, 1); grid.Children.Add(scroll); context.ViewerContent = grid;
            BullDocument opened = null;
            try
            {
                opened = await Task.Run(() => BullDocument.Open(path));
                if (closed || token != generation) { opened.Dispose(); return; }
                document = opened; opened = null;
                ShowPage(0);
            }
            catch (Exception ex)
            {
                if (!closed && token == generation) { status.Text = "Cannot open collection: " + ex.Message; context.IsBusy = false; }
            }
            finally { if (opened != null) opened.Dispose(); }
        }
        private async void ShowPage(int index)
        {
            var current = document;
            if (closed || current == null || index < 0 || index >= current.Count) return;
            int token = ++generation; page = index;
            previous.IsEnabled = next.IsEnabled = false; status.Text = "Loading page…"; context.IsBusy = true;
            try
            {
                BitmapImage bitmap = await Task.Run(() => {
                    byte[] bytes = current.ReadPage(index);
                    using (var stream = new MemoryStream(bytes, false))
                    {
                        var result = new BitmapImage(); result.BeginInit(); result.CacheOption = BitmapCacheOption.OnLoad;
                        result.StreamSource = stream; result.EndInit(); result.Freeze(); return result;
                    }
                });
                if (closed || token != generation) return;
                image.Source = bitmap; status.Text = string.Format("{0} / {1}", index + 1, current.Count);
                previous.IsEnabled = index > 0; next.IsEnabled = index + 1 < current.Count; context.IsBusy = false;
            }
            catch (Exception ex)
            {
                if (!closed && token == generation) { image.Source = null; status.Text = "Cannot open page: " + ex.Message; previous.IsEnabled = index > 0; next.IsEnabled = index + 1 < current.Count; context.IsBusy = false; }
            }
        }
        public void Cleanup()
        {
            closed = true; generation++;
            var old = document; document = null;
            if (old != null) Task.Run(() => old.Dispose());
            image = null; previous = next = null; status = null; context = null;
        }
    }
}
